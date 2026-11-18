//! Windows' own spell checker (the ISpellChecker interface Windows 8 and later provide, with the dictionaries the
//! user has installed), used by the de-identification suggestions pass to tell ordinary words from possible names.
//! Nothing is downloaded. Where no dictionary is available every word counts as unknown, and the pass falls back on
//! capital letters alone.

/// A dictionary for one language.
pub struct Dictionary {
    #[cfg(windows)]
    checker: Option<windows::Win32::Globalization::ISpellChecker>,
    /// The language tag in use ("en-US"), or none.
    pub language: Option<String>,
}

#[cfg(windows)]
mod imp {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Globalization::{GetUserDefaultLocaleName, ISpellChecker, ISpellCheckerFactory, ISpellingError, SpellCheckerFactory};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};

    pub fn user_locale() -> Option<String> {
        let mut buf = [0u16; 85];
        let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
        (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
    }

    pub fn open(candidates: &[String]) -> Option<(ISpellChecker, String)> {
        unsafe {
            // This thread may already be set up for COM in another mode; either way the factory can be made.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let factory: ISpellCheckerFactory = CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER).ok()?;
            for tag in candidates {
                let h = HSTRING::from(tag.as_str());
                if factory.IsSupported(PCWSTR(h.as_ptr())).map(|b| b.as_bool()).unwrap_or(false) {
                    if let Ok(c) = factory.CreateSpellChecker(PCWSTR(h.as_ptr())) {
                        return Some((c, tag.clone()));
                    }
                }
            }
            None
        }
    }

    pub fn known(c: &ISpellChecker, word: &str) -> bool {
        let h = HSTRING::from(word);
        unsafe {
            let Ok(errors) = c.Check(PCWSTR(h.as_ptr())) else { return false };
            let mut first: Option<ISpellingError> = None;
            // S_OK with an error means the word is not known; S_FALSE (nothing) means it is.
            let hr = errors.Next(&mut first);
            !(hr.0 == 0 && first.is_some())
        }
    }
}

impl Dictionary {
    /// The dictionary for an ISO 639-1 language ("en"), or the user's own language when none is given.
    pub fn for_language(lang: Option<&str>) -> Dictionary {
        #[cfg(windows)]
        {
            let user = imp::user_locale();
            let lang = lang.map(str::to_lowercase).or_else(|| user.as_deref().and_then(|u| u.split('-').next()).map(str::to_lowercase)).unwrap_or_else(|| "en".into());
            let mut candidates: Vec<String> = Vec::new();
            if let Some(u) = user.as_ref().filter(|u| u.to_lowercase().starts_with(&format!("{lang}-"))) {
                candidates.push(u.clone());
            }
            match lang.as_str() {
                "en" => candidates.extend(["en-US", "en-GB", "en-CA", "en-AU"].map(String::from)),
                "pt" => candidates.extend(["pt-PT", "pt-BR"].map(String::from)),
                "uk" => candidates.push("uk-UA".into()),
                "sv" => candidates.push("sv-SE".into()),
                "da" => candidates.push("da-DK".into()),
                "cs" => candidates.push("cs-CZ".into()),
                "el" => candidates.push("el-GR".into()),
                "et" => candidates.push("et-EE".into()),
                "sl" => candidates.push("sl-SI".into()),
                _ => {}
            }
            candidates.push(format!("{lang}-{}", lang.to_uppercase()));
            candidates.push(lang.clone());
            match imp::open(&candidates) {
                Some((c, tag)) => Dictionary { checker: Some(c), language: Some(tag) },
                None => Dictionary { checker: None, language: None },
            }
        }
        #[cfg(not(windows))]
        {
            let _ = lang;
            Dictionary { language: None }
        }
    }

    /// Whether the word is in the dictionary as written.
    pub fn known(&self, word: &str) -> bool {
        #[cfg(windows)]
        {
            self.checker.as_ref().is_some_and(|c| imp::known(c, word))
        }
        #[cfg(not(windows))]
        {
            let _ = word;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows ships an English dictionary; this checks the interface is called the right way round.
    #[test]
    #[cfg(windows)]
    fn ordinary_words_are_known_and_nonsense_is_not() {
        let d = Dictionary::for_language(Some("en"));
        if d.language.is_none() {
            eprintln!("no English dictionary on this computer; skipped");
            return;
        }
        assert!(d.known("house") && d.known("walked"));
        assert!(!d.known("qzxvbnm") && !d.known("brookfieldton"));
    }
}
