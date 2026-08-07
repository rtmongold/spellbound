# spellbound

[![CI](https://github.com/rtmongold/spellbound/actions/workflows/ci.yml/badge.svg)](https://github.com/rtmongold/spellbound/actions/workflows/ci.yml)

Native spell checking with a small Rust API.

This is a **maintained fork** of [euclio/spellbound](https://github.com/euclio/spellbound)
(last upstream commit 2020).

| Platform | API                |
| -------- | ------------------ |
| macOS    | [`NSSpellChecker`] |
| Windows  | [`ISpellChecker`]  |
| *nix     | [`hunspell`] |

[`ISpellChecker`]: https://docs.microsoft.com/en-us/windows/desktop/api/spellcheck/nn-spellcheck-ispellchecker
[`NSSpellChecker`]: https://developer.apple.com/documentation/appkit/nsspellchecker
[`hunspell`]: https://hunspell.github.io/

## Example

```rust
use spellbound::Checker;

fn main() -> Result<(), spellbound::Error> {
    let checker = Checker::new()?;
    // Or: Checker::with_locale("en-US")?;

    for err in checker.check("I beleeve I can fly") {
        println!("{} @ {}..{}", err.text(), err.start(), err.end());
        for suggestion in checker.suggest(err.text()) {
            println!("  → {suggestion}");
        }
    }
    Ok(())
}
```

`Checker::new()` defaults to English (`en_US` / `en_GB` on Linux, `en-US` on Windows). Use `with_locale` for another language. Locales may be written as `en_US` or `en-US`.

Unknown or unsupported locales behave differently by platform:

- **Linux:** missing dictionary / unknown locale → `Error::Unavailable`
- **macOS:** empty locale → `Error::Unavailable`; unknown tags may still create a checker (system fallback)
- **Windows:** unsupported language tag → `Error::Unavailable`

## Threading

macOS serializes access to the shared `NSSpellChecker`. Do not assume `Checker` is `Send` / `Sync` across platforms.

## Linux

Needs a hunspell dictionary on disk (default search includes `/usr/share/hunspell`). Example:

- Arch: `pacman -S hunspell hunspell-en_us`
- Debian/Ubuntu: `apt install libhunspell-dev hunspell-en-us`

Without a dictionary, `Checker::new()` returns `Error::Unavailable`.

## License
MIT OR Apache-2.0

## Credits

Originally by [Andy Russell](https://github.com/euclio). Maintained in this fork by Robert Mongold.