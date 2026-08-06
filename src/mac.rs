use crate::Error;

use std::ops::Deref;
use std::slice;
use std::str;
use std::sync::{Mutex, OnceLock};

use cocoa::{
    appkit::NSSpellChecker,
    base::{id, nil, NO},
    foundation::{NSInteger, NSNotFound, NSString, NSUInteger},
};

fn checker() -> &'static Mutex<NSSpellCheckerWrapper> {
    static CHECKER: OnceLock<Mutex<NSSpellCheckerWrapper>> = OnceLock::new();
    CHECKER.get_or_init(|| {
        Mutex::new(unsafe { NSSpellCheckerWrapper(NSSpellChecker::sharedSpellChecker(nil)) })
    })
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
        })
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
            ns_text: unsafe { NSString::alloc(nil).init_str(text) },
            ns_offset: 0,
            original: text.to_owned(),
            byte_cursor: 0,
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
}

impl Iterator for SpellcheckIter {
    type Item = SpellingError;

    fn next(&mut self) -> Option<Self::Item> {
        let (range, _) =
            unsafe {
                checker().lock().unwrap()
                .checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                    self.ns_text,
                    self.ns_offset as NSInteger,
                    nil,
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
