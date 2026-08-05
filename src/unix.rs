use crate::Error;

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use hunspell_sys::{Hunhandle, Hunspell_add, Hunspell_create, Hunspell_destroy, Hunspell_spell};

const DICT_DIRS: &[&str] = &[
    "/usr/share/hunspell",
    "/usr/share/myspell/dicts",
    "/usr/share/myspell",
    "/usr/local/share/hunspell",
];

const DEFAULT_LOCALES: &[&str] = &["en_US", "en_GB"];

fn find_dictionary() -> Option<(PathBuf, PathBuf)> {
    for dir in DICT_DIRS {
        for locale in DEFAULT_LOCALES {
            let aff = Path::new(dir).join(format!("{locale}.aff"));
            let dic = Path::new(dir).join(format!("{locale}.dic"));
            if aff.is_file() && dic.is_file() {
                return Some((aff, dic));
            }
        }
    }
    None
}

#[derive(Debug)]
pub struct Checker {
    hunspell: *mut Hunhandle,
}

impl Checker {
    pub fn new() -> Result<Self, Error> {
        let (aff, dic) = find_dictionary().ok_or(Error::Unavailable)?;

        let hunspell = unsafe {
            Hunspell_create(
                aff.as_os_str().as_bytes().as_ptr() as *const i8,
                dic.as_os_str().as_bytes().as_ptr() as *const i8,
            )
        };

        Ok(Checker { hunspell })
    }

    pub fn check<'a, 'b: 'a>(
        &'b mut self,
        text: &'a str,
    ) -> impl Iterator<Item = SpellingError> + 'a {
        let hunspell = self.hunspell;

        words(text).filter_map(move |(start, end, word)| {
            let cstr = CString::new(word).ok()?;
            let ok = unsafe { Hunspell_spell(hunspell, cstr.as_bytes_with_nul().as_ptr() as *const i8) } != 0;
            if ok {
                None
            } else {
                Some(SpellingError {
                    text: word.to_owned(),
                    start,
                    end,
                })
            }
        })
    }

    pub fn ignore(&mut self, word: &str) {
        let cstr = CString::new(word).unwrap();

        unsafe { Hunspell_add(self.hunspell, cstr.as_bytes_with_nul().as_ptr() as *const i8) };
    }
}

impl Drop for Checker {
    fn drop(&mut self) {
        unsafe {
            Hunspell_destroy(self.hunspell);
        }
    }
}

pub struct SpellingError {
    text: String,
    start: usize,
    end: usize,
}

impl SpellingError {
    pub fn text(&self) -> &str { &self.text }
    pub fn start(&self) -> usize { self.start }
    pub fn end(&self) -> usize { self.end }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\''
}

fn words(text: &str) -> impl Iterator<Item = (usize, usize, &str)> {
    let mut words = Vec::new();
    let mut chars = text.char_indices().peekable();
    
    while let Some((start, c)) = chars.next() {
        if !is_word_char(c) {
            continue;
        }
    
        let mut end = start + c.len_utf8();
        while let Some(&(i, next)) = chars.peek() {
            if !is_word_char(next) {
                break;
            }
            end = i + next.len_utf8();
            chars.next();
        }
        words.push((start, end, &text[start..end]));
    }
    words.into_iter()
}
