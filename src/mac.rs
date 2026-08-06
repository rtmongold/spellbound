use crate::Error;

use std::ops::Deref;
use std::slice;
use std::str;
use std::sync::{Mutex, OnceLock};

use cocoa::{
    appkit::NSSpellChecker,
    base::{id, nil, NO},
    foundation::{NSInteger, NSNotFound, NSRange, NSString, NSUInteger},
};
use objc::{msg_send, sel, sel_impl};

fn checker() -> &'static Mutex<NSSpellCheckerWrapper> {
    static CHECKER: OnceLock<Mutex<NSSpellCheckerWrapper>> = OnceLock::new();
    CHECKER.get_or_init(|| {
        Mutex::new(unsafe { NSSpellCheckerWrapper(NSSpellChecker::sharedSpellChecker(nil)) })
    })
}

fn ns_string(s: &str) -> id {
    unsafe { NSString::alloc(nil).init_str(s) }
}

fn language_id(language: &Option<String>) -> id {
    match language {
        Some(tag) => ns_string(tag),
        None => nil,
    }
}

fn nsarray_to_strings(array: id, max: usize) -> Vec<String> {
    if array.is_null() {
        return Vec::new();
    }
    unsafe {
        let count: NSUInteger = msg_send![array, count];
        let take = (count as usize).min(max);
        let mut out = Vec::with_capacity(take);
        for i in 0..take {
            let item: id = msg_send![array, objectAtIndex: i];
            if item.is_null() {
                continue;
            }
            let bytes = item.UTF8String() as *const u8;
            let len = item.len();
            if let Ok(s) = str::from_utf8(slice::from_raw_parts(bytes, len)) {
                out.push(s.to_owned());
            }
        }
        out
    }
}

/// `NSSpellChecker` is not thread safe. It should only be used from one thread, or it will cause
/// spurious `EXC_BAD_ACCESS` errors. If access to it is synchronized, however, it should be safe
/// to send across threads.
struct NSSpellCheckerWrapper(id);

unsafe impl Send for NSSpellCheckerWrapper {}

impl Deref for NSSpellCheckerWrapper {
    type Target = id;

    fn deref(&self) -> &id {
        &self.0
    }
}

#[derive(Debug)]
pub struct Checker {
    document_tag: NSInteger,
    /// BCP-47 tag (`en-US`), or `None` for the system default language.
    language: Option<String>,
}

impl Drop for Checker {
    fn drop(&mut self) {
        unsafe {
            checker()
                .lock()
                .unwrap()
                .closeSpellDocumentWithTag(self.document_tag)
        };
    }
}

impl Checker {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            document_tag: unsafe { NSSpellChecker::uniqueSpellDocumentTag(nil) },
            language: None,
        })
    }

    pub fn with_locale(_hunspell: &str, bcp47: &str) -> Result<Self, Error> {
        // Soft validation: reject empty; unavailable languages still may “work”
        // with system fallback — zz_ZZ is only strictly Err on Unix.
        if bcp47.is_empty() {
            return Err(Error::Unavailable);
        }
        Ok(Self {
            document_tag: unsafe { NSSpellChecker::uniqueSpellDocumentTag(nil) },
            language: Some(bcp47.to_owned()),
        })
    }
    pub fn suggest(&self, word: &str) -> Vec<String> {
        const MAX: usize = 10;
        if word.is_empty() {
            return Vec::new();
        }

        let ns_word = ns_string(word);
        let lang = language_id(&self.language);
        let length: NSUInteger = unsafe { msg_send![ns_word, length] };
        let range = NSRange {
            location: 0,
            length,
        };

        let guesses: id = unsafe {
            let guard = checker().lock().unwrap();
            msg_send![
                **guard,
                guessesForWordRange: range
                inString: ns_word
                language: lang
                inSpellDocumentWithTag: self.document_tag
            ]
        };
        nsarray_to_strings(guesses, MAX)
    }

    pub fn ignore(&mut self, word: &str) {
        let word = unsafe { NSString::alloc(nil).init_str(word) };
        unsafe {
            checker()
                .lock()
                .unwrap()
                .ignoreWord_inSpellDocumentWithTag(word, self.document_tag)
        };
    }

    pub fn check(&mut self, text: &str) -> impl Iterator<Item = SpellingError> {
        SpellcheckIter {
            document_tag: self.document_tag,
            ns_text: ns_string(text),
            ns_offset: 0,
            original: text.to_owned(),
            byte_cursor: 0,
            language: self.language.clone(),
        }
    }
}

#[derive(Debug)]
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

struct SpellcheckIter {
    document_tag: NSInteger,
    ns_text: id, /* NSString */
    ns_offset: NSUInteger,
    original: String,
    byte_cursor: usize,
    language: Option<String>,
}

impl Iterator for SpellcheckIter {
    type Item = SpellingError;

    fn next(&mut self) -> Option<Self::Item> {
        let lang = language_id(&self.language);
        let (range, _) =
            unsafe {
                checker().lock().unwrap()
                .checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                    self.ns_text,
                    self.ns_offset as NSInteger,
                    lang,
                    NO,
                    self.document_tag,
                )
            };

        if range.location == NSNotFound as NSUInteger {
            return None;
        };

        let misspelling = unsafe {
            let misspelling = self.ns_text.substringWithRange(range);
            let misspelling_bytes = misspelling.UTF8String() as *const u8;
            str::from_utf8(slice::from_raw_parts(misspelling_bytes, misspelling.len())).unwrap()
        };
        let rest = &self.original[self.byte_cursor..];
        let rel = rest.find(misspelling)?;
        let start = self.byte_cursor + rel;
        let end = start + misspelling.len();
        self.byte_cursor = end;
        self.ns_offset = range.location + range.length;

        Some(SpellingError {
            text: misspelling.to_owned(),
            start,
            end,
        })
    }
}
