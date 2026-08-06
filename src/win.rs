use crate::Error;

use std::ffi::OsStr;
use std::fmt::{self, Debug};
use std::iter;
use std::ops::Deref;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{self, NonNull};

use winapi::{
    shared::{
        ntdef::ULONG,
        winerror::{SUCCEEDED, S_FALSE, S_OK},
        wtypesbase::CLSCTX_INPROC_SERVER,
    },
    um::{
        combaseapi::{CoCreateInstance, CoInitializeEx, CoTaskMemFree},
        objbase::COINIT_MULTITHREADED,
        spellcheck::{
            IEnumSpellingError, ISpellChecker, ISpellCheckerFactory, SpellCheckerFactory,
        },
        unknwnbase::IUnknown,
    },
    Class, Interface,
};

struct ComPtr<T>(NonNull<T>);

impl<T> ComPtr<T> {
    fn new(p: *mut T) -> ComPtr<T>
    where
        T: Interface,
    {
        ComPtr(NonNull::new(p).unwrap())
    }
}

impl<T> Deref for ComPtr<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.0.as_ptr() }
    }
}

impl<T> Debug for ComPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("ComPtr")
            .field(&format_args!("{:p}", self.0.as_ptr()))
            .finish()
    }
}

impl<T> Drop for ComPtr<T> {
    fn drop(&mut self) {
        unsafe {
            let unknown = self.0.as_ptr() as *mut IUnknown;
            (*unknown).Release();
        }
    }
}

fn wide_string(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(iter::once(0)).collect()
}

fn utf16_offset_to_utf8(s: &str, utf16_units: usize) -> usize {
    let mut units = 0;
    for (byte_idx, ch) in s.char_indices() {
        if units >= utf16_units {
            return byte_idx;
        }
        units += ch.len_utf16();
    }
    s.len()
}

fn create_factory() -> Result<ComPtr<ISpellCheckerFactory>, Error> {
    let hr = unsafe { CoInitializeEx(ptr::null_mut(), COINIT_MULTITHREADED) };
    if hr != S_OK && hr != S_FALSE {
        return Err(Error::Unavailable);
    }

    let mut obj = ptr::null_mut();
    let hr = unsafe {
        CoCreateInstance(
            &SpellCheckerFactory::uuidof(),
            ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &ISpellCheckerFactory::uuidof(),
            &mut obj,
        )
    };
    if !SUCCEEDED(hr) {
        return Err(Error::Unavailable);
    }
    Ok(ComPtr::new(obj as *mut ISpellCheckerFactory))
}

fn open_for_language(bcp47: &str) -> Result<ComPtr<ISpellChecker>, Error> {
    let factory = create_factory()?;
    let lang = wide_string(bcp47);
    let mut checker = ptr::null_mut();
    let hr = unsafe { (*factory).CreateSpellChecker(lang.as_ptr(), &mut checker) };
    if !SUCCEEDED(hr) {
        return Err(Error::Unavailable);
    }
    Ok(ComPtr::new(checker))
}

unsafe fn wide_ptr_to_string(p: *mut u16) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let mut len = 0usize;
    while *p.add(len) != 0 {
        len += 1;
    }
    String::from_utf16(std::slice::from_raw_parts(p, len)).ok()
}

#[derive(Debug)]
pub struct Checker {
    checker: ComPtr<ISpellChecker>,
}

impl Checker {
    pub fn new() -> Result<Self, Error> {
        Self::with_locale("en_US", "en-US")
    }

    pub fn with_locale(_hunspell: &str, bcp47: &str) -> Result<Self, Error> {
        Ok(Checker {
            checker: open_for_language(bcp47)?,
        })
    }

    pub fn suggest(&self, word: &str) -> Vec<String> {
        const MAX: usize = 10;

        if word.is_empty() {
            return Vec::new();
        }

        let wide = wide_string(word);
        let mut enum_str = ptr::null_mut();
        let hr = unsafe { (*self.checker).Suggest(wide.as_ptr(), &mut enum_str) };
        if !SUCCEEDED(hr) || enum_str.is_null() {
            return Vec::new();
        }
        let enum_str = ComPtr::new(enum_str);

        let mut out = Vec::new();
        while out.len() < MAX {
            let mut item: *mut u16 = ptr::null_mut();
            let mut fetched: ULONG = 0;
            let hr = unsafe { (*enum_str).Next(1, &mut item, &mut fetched) };
            if hr == S_FALSE || fetched == 0 || item.is_null() {
                break;
            }
            if let Some(s) = unsafe { wide_ptr_to_string(item) } {
                out.push(s);
            }
            unsafe { CoTaskMemFree(item as *mut _) };
            if !SUCCEEDED(hr) && hr != S_FALSE {
                break;
            }
        }
        out
    }

    pub fn check(&mut self, text: &str) -> impl Iterator<Item = SpellingError> {
        if text.is_empty() {
            return ErrorIter {
                original: String::new(),
                text: vec![],
                iter: None,
            };
        }

        let original = text.to_owned();
        let wide = wide_string(text);
        let mut errors = ptr::null_mut();
        let hr = unsafe { (*self.checker).ComprehensiveCheck(wide.as_ptr(), &mut errors) };
        if !SUCCEEDED(hr) {
            return ErrorIter {
                original,
                text: wide,
                iter: None,
            };
        }
        let errors = ComPtr::new(errors);

        ErrorIter {
            original,
            text: wide,
            iter: Some(errors),
        }
    }

    pub fn ignore(&mut self, word: &str) {
        if word.is_empty() {
            return;
        }

        let word = wide_string(word);
        let hr = unsafe { (*self.checker).Ignore(word.as_ptr()) };
        if !SUCCEEDED(hr) {
            return;
        }
    }
}

struct ErrorIter {
    original: String,
    text: Vec<u16>,
    iter: Option<ComPtr<IEnumSpellingError>>,
}

impl Iterator for ErrorIter {
    type Item = SpellingError;

    fn next(&mut self) -> Option<SpellingError> {
        let iter = self.iter.as_ref()?;

        let mut err = ptr::null_mut();
        if unsafe { (*iter).Next(&mut err) } != S_FALSE {
            let err = ComPtr::new(err);

            let mut start = 0;
            let mut length = 0;

            unsafe {
                (*err).get_Length(&mut length);
                (*err).get_StartIndex(&mut start);
            }

            let utf16_start = start as usize;
            let utf16_len = length as usize;

            let err_text =
                String::from_utf16(&self.text[utf16_start..utf16_start + utf16_len]).ok()?;

            let byte_start = utf16_offset_to_utf8(&self.original, utf16_start);
            let byte_end = utf16_offset_to_utf8(&self.original, utf16_start + utf16_len);

            return Some(SpellingError {
                text: err_text,
                start: byte_start,
                end: byte_end,
            });
        } else {
            None
        }
    }
}

pub struct SpellingError {
    text: String,
    start: usize,
    end: usize,
}

impl SpellingError {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn start(&self) -> usize {
        self.start
    }
    pub fn end(&self) -> usize {
        self.end
    }
}
