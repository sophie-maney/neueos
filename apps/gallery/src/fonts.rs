//! The UI font candidates from PLAN §4 (task 2.10, open question 2), bundled for the
//! gallery's Fonts page only. All are Regular weight under the SIL Open Font Licence 1.1
//! (licence texts next to the files in `fonts/`). Fira Sans, the current placeholder, is
//! already the UI font, so it isn't repeated here.

/// A candidate: its family name and its font file.
#[derive(Debug, Clone, Copy)]
pub struct Candidate {
    pub family: &'static str,
    pub data: Option<&'static [u8]>,
}

/// Every candidate, placeholder first.
pub const CANDIDATES: &[Candidate] = &[
    Candidate {
        family: neue_kit::tokens::typography::UI_FAMILY,
        data: None,
    },
    Candidate {
        family: "Source Sans 3",
        data: Some(include_bytes!("../fonts/SourceSans3-Regular.ttf")),
    },
    Candidate {
        family: "IBM Plex Sans",
        data: Some(include_bytes!("../fonts/IBMPlexSans-Regular.ttf")),
    },
    Candidate {
        family: "Noto Sans",
        data: Some(include_bytes!("../fonts/NotoSans-Regular.ttf")),
    },
];

/// The font files to register through `neue_kit::Settings::extra_fonts`.
pub fn extra_fonts() -> Vec<&'static [u8]> {
    CANDIDATES.iter().filter_map(|c| c.data).collect()
}
