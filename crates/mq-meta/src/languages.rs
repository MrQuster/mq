//! Minecraft all-languages catalog (80+ locales) and asset resolution.

use serde::{Deserialize, Serialize};

/// Information about a Minecraft game localization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct McLocaleInfo {
    /// Minecraft locale code used in assets and `options.txt` (e.g. `"uk_ua"`).
    pub code: &'static str,
    /// Native language name in the target language (e.g. `"Українська"`).
    pub name: &'static str,
    /// Region / country in the target language (e.g. `"Україна"`).
    pub region: &'static str,
    /// Standard English display name (e.g. `"Ukrainian"`).
    pub english_name: &'static str,
}

impl McLocaleInfo {
    /// Full display title suitable for dropdowns (e.g. `"Українська (Україна) [uk_ua]"`).
    pub fn display_label(&self) -> String {
        format!("{} ({}) [{}]", self.name, self.region, self.code)
    }
}

/// Catalog of 80+ Minecraft Java Edition locales.
pub const ALL_MC_LOCALES: &[McLocaleInfo] = &[
    McLocaleInfo {
        code: "uk_ua",
        name: "Українська",
        region: "Україна",
        english_name: "Ukrainian",
    },
    McLocaleInfo {
        code: "ru_ru",
        name: "Русский",
        region: "Россия",
        english_name: "Russian",
    },
    McLocaleInfo {
        code: "en_us",
        name: "English",
        region: "US",
        english_name: "English (US)",
    },
    McLocaleInfo {
        code: "en_gb",
        name: "English",
        region: "UK",
        english_name: "English (UK)",
    },
    McLocaleInfo {
        code: "en_ca",
        name: "English",
        region: "Canada",
        english_name: "English (Canada)",
    },
    McLocaleInfo {
        code: "en_au",
        name: "English",
        region: "Australia",
        english_name: "English (Australia)",
    },
    McLocaleInfo {
        code: "en_nz",
        name: "English",
        region: "New Zealand",
        english_name: "English (New Zealand)",
    },
    McLocaleInfo {
        code: "pl_pl",
        name: "Polski",
        region: "Polska",
        english_name: "Polish",
    },
    McLocaleInfo {
        code: "de_de",
        name: "Deutsch",
        region: "Deutschland",
        english_name: "German",
    },
    McLocaleInfo {
        code: "de_at",
        name: "Deutsch",
        region: "Österreich",
        english_name: "German (Austria)",
    },
    McLocaleInfo {
        code: "de_ch",
        name: "Deutsch",
        region: "Schweiz",
        english_name: "German (Switzerland)",
    },
    McLocaleInfo {
        code: "fr_fr",
        name: "Français",
        region: "France",
        english_name: "French",
    },
    McLocaleInfo {
        code: "fr_ca",
        name: "Français",
        region: "Canada",
        english_name: "French (Canada)",
    },
    McLocaleInfo {
        code: "es_es",
        name: "Español",
        region: "España",
        english_name: "Spanish (Spain)",
    },
    McLocaleInfo {
        code: "es_mx",
        name: "Español",
        region: "México",
        english_name: "Spanish (Mexico)",
    },
    McLocaleInfo {
        code: "es_ar",
        name: "Español",
        region: "Argentina",
        english_name: "Spanish (Argentina)",
    },
    McLocaleInfo {
        code: "es_cl",
        name: "Español",
        region: "Chile",
        english_name: "Spanish (Chile)",
    },
    McLocaleInfo {
        code: "es_uy",
        name: "Español",
        region: "Uruguay",
        english_name: "Spanish (Uruguay)",
    },
    McLocaleInfo {
        code: "es_ve",
        name: "Español",
        region: "Venezuela",
        english_name: "Spanish (Venezuela)",
    },
    McLocaleInfo {
        code: "be_by",
        name: "Беларуская",
        region: "Беларусь",
        english_name: "Belarusian",
    },
    McLocaleInfo {
        code: "kk_kz",
        name: "Қазақша",
        region: "Қазақстан",
        english_name: "Kazakh",
    },
    McLocaleInfo {
        code: "zh_cn",
        name: "简体中文",
        region: "中国",
        english_name: "Simplified Chinese",
    },
    McLocaleInfo {
        code: "zh_tw",
        name: "繁體中文",
        region: "台灣",
        english_name: "Traditional Chinese",
    },
    McLocaleInfo {
        code: "zh_hk",
        name: "繁體中文",
        region: "香港",
        english_name: "Traditional Chinese (HK)",
    },
    McLocaleInfo {
        code: "ja_jp",
        name: "日本語",
        region: "日本",
        english_name: "Japanese",
    },
    McLocaleInfo {
        code: "ko_kr",
        name: "한국어",
        region: "대한민국",
        english_name: "Korean",
    },
    McLocaleInfo {
        code: "it_it",
        name: "Italiano",
        region: "Italia",
        english_name: "Italian",
    },
    McLocaleInfo {
        code: "pt_br",
        name: "Português",
        region: "Brasil",
        english_name: "Portuguese (Brazil)",
    },
    McLocaleInfo {
        code: "pt_pt",
        name: "Português",
        region: "Portugal",
        english_name: "Portuguese (Portugal)",
    },
    McLocaleInfo {
        code: "tr_tr",
        name: "Türkçe",
        region: "Türkiye",
        english_name: "Turkish",
    },
    McLocaleInfo {
        code: "cs_cz",
        name: "Čeština",
        region: "Česko",
        english_name: "Czech",
    },
    McLocaleInfo {
        code: "nl_nl",
        name: "Nederlands",
        region: "Nederland",
        english_name: "Dutch",
    },
    McLocaleInfo {
        code: "nl_be",
        name: "Nederlands",
        region: "België",
        english_name: "Dutch (Belgium)",
    },
    McLocaleInfo {
        code: "sv_se",
        name: "Svenska",
        region: "Sverige",
        english_name: "Swedish",
    },
    McLocaleInfo {
        code: "da_dk",
        name: "Dansk",
        region: "Danmark",
        english_name: "Danish",
    },
    McLocaleInfo {
        code: "fi_fi",
        name: "Suomi",
        region: "Suomi",
        english_name: "Finnish",
    },
    McLocaleInfo {
        code: "nb_no",
        name: "Norsk bokmål",
        region: "Norge",
        english_name: "Norwegian Bokmål",
    },
    McLocaleInfo {
        code: "nn_no",
        name: "Norsk nynorsk",
        region: "Noreg",
        english_name: "Norwegian Nynorsk",
    },
    McLocaleInfo {
        code: "el_gr",
        name: "Ελληνικά",
        region: "Ελλάδα",
        english_name: "Greek",
    },
    McLocaleInfo {
        code: "hu_hu",
        name: "Magyar",
        region: "Magyarország",
        english_name: "Hungarian",
    },
    McLocaleInfo {
        code: "ro_ro",
        name: "Română",
        region: "România",
        english_name: "Romanian",
    },
    McLocaleInfo {
        code: "bg_bg",
        name: "Български",
        region: "България",
        english_name: "Bulgarian",
    },
    McLocaleInfo {
        code: "sk_sk",
        name: "Slovenčina",
        region: "Slovensko",
        english_name: "Slovak",
    },
    McLocaleInfo {
        code: "sl_si",
        name: "Slovenščina",
        region: "Slovenija",
        english_name: "Slovenian",
    },
    McLocaleInfo {
        code: "hr_hr",
        name: "Hrvatski",
        region: "Hrvatska",
        english_name: "Croatian",
    },
    McLocaleInfo {
        code: "sr_sp",
        name: "Srpski",
        region: "Srbija",
        english_name: "Serbian",
    },
    McLocaleInfo {
        code: "lt_lt",
        name: "Lietuvių",
        region: "Lietuva",
        english_name: "Lithuanian",
    },
    McLocaleInfo {
        code: "lv_lv",
        name: "Latviešu",
        region: "Latvija",
        english_name: "Latvian",
    },
    McLocaleInfo {
        code: "et_ee",
        name: "Eesti",
        region: "Eesti",
        english_name: "Estonian",
    },
    McLocaleInfo {
        code: "he_il",
        name: "עברית",
        region: "ישראל",
        english_name: "Hebrew",
    },
    McLocaleInfo {
        code: "ar_sa",
        name: "العربية",
        region: "السعودية",
        english_name: "Arabic",
    },
    McLocaleInfo {
        code: "fa_ir",
        name: "فارسی",
        region: "ایران",
        english_name: "Persian",
    },
    McLocaleInfo {
        code: "hi_in",
        name: "हिन्दी",
        region: "भारत",
        english_name: "Hindi",
    },
    McLocaleInfo {
        code: "th_th",
        name: "ไทย",
        region: "ประเทศไทย",
        english_name: "Thai",
    },
    McLocaleInfo {
        code: "vi_vn",
        name: "Tiếng Việt",
        region: "Việt Nam",
        english_name: "Vietnamese",
    },
    McLocaleInfo {
        code: "id_id",
        name: "Bahasa Indonesia",
        region: "Indonesia",
        english_name: "Indonesian",
    },
    McLocaleInfo {
        code: "ms_my",
        name: "Bahasa Melayu",
        region: "Malaysia",
        english_name: "Malay",
    },
    McLocaleInfo {
        code: "fil_ph",
        name: "Filipino",
        region: "Pilipinas",
        english_name: "Filipino",
    },
    McLocaleInfo {
        code: "az_az",
        name: "Azərbaycanca",
        region: "Azərbaycan",
        english_name: "Azerbaijani",
    },
    McLocaleInfo {
        code: "hy_am",
        name: "Հայերեն",
        region: "Հայաստան",
        english_name: "Armenian",
    },
    McLocaleInfo {
        code: "ka_ge",
        name: "ქართული",
        region: "საქართველო",
        english_name: "Georgian",
    },
    McLocaleInfo {
        code: "mn_mn",
        name: "Монгол",
        region: "Монгол",
        english_name: "Mongolian",
    },
    McLocaleInfo {
        code: "uz_uz",
        name: "Oʻzbekcha",
        region: "Oʻzbekiston",
        english_name: "Uzbek",
    },
    McLocaleInfo {
        code: "tt_ru",
        name: "Татарча",
        region: "Россия",
        english_name: "Tatar",
    },
    McLocaleInfo {
        code: "ba_ru",
        name: "Башҡортса",
        region: "Россия",
        english_name: "Bashkir",
    },
    McLocaleInfo {
        code: "sah_sah",
        name: "Сахалыы",
        region: "Россия",
        english_name: "Yakut",
    },
    McLocaleInfo {
        code: "cv_ru",
        name: "Чӑвашла",
        region: "Россия",
        english_name: "Chuvash",
    },
    McLocaleInfo {
        code: "ca_es",
        name: "Català",
        region: "Catalunya",
        english_name: "Catalan",
    },
    McLocaleInfo {
        code: "eu_es",
        name: "Euskara",
        region: "Euskal Herria",
        english_name: "Basque",
    },
    McLocaleInfo {
        code: "gl_es",
        name: "Galego",
        region: "Galiza",
        english_name: "Galician",
    },
    McLocaleInfo {
        code: "cy_gb",
        name: "Cymraeg",
        region: "Cymru",
        english_name: "Welsh",
    },
    McLocaleInfo {
        code: "ga_ie",
        name: "Gaeilge",
        region: "Éire",
        english_name: "Irish",
    },
    McLocaleInfo {
        code: "gd_gb",
        name: "Gàidhlig",
        region: "Alba",
        english_name: "Scottish Gaelic",
    },
    McLocaleInfo {
        code: "br_fr",
        name: "Brezhoneg",
        region: "Breizh",
        english_name: "Breton",
    },
    McLocaleInfo {
        code: "oc_fr",
        name: "Occitan",
        region: "Occitània",
        english_name: "Occitan",
    },
    McLocaleInfo {
        code: "af_za",
        name: "Afrikaans",
        region: "Suid-Afrika",
        english_name: "Afrikaans",
    },
    McLocaleInfo {
        code: "sw_ke",
        name: "Kiswahili",
        region: "Kenya",
        english_name: "Swahili",
    },
    McLocaleInfo {
        code: "yo_ng",
        name: "Yorùbá",
        region: "Nàìjíríà",
        english_name: "Yoruba",
    },
    McLocaleInfo {
        code: "ig_ng",
        name: "Asụsụ Igbo",
        region: "Naịjirịa",
        english_name: "Igbo",
    },
    McLocaleInfo {
        code: "ha_ng",
        name: "Hausa",
        region: "Najeriya",
        english_name: "Hausa",
    },
    McLocaleInfo {
        code: "is_is",
        name: "Íslenska",
        region: "Ísland",
        english_name: "Icelandic",
    },
    McLocaleInfo {
        code: "fo_fo",
        name: "Føroyskt",
        region: "Føroyar",
        english_name: "Faroese",
    },
    McLocaleInfo {
        code: "kl_gl",
        name: "Kalaallisut",
        region: "Kalaallit Nunaat",
        english_name: "Greenlandic",
    },
    McLocaleInfo {
        code: "lb_lu",
        name: "Lëtzebuergesch",
        region: "Lëtzebuerg",
        english_name: "Luxembourgish",
    },
    McLocaleInfo {
        code: "mt_mt",
        name: "Malti",
        region: "Malta",
        english_name: "Maltese",
    },
    McLocaleInfo {
        code: "sq_al",
        name: "Shqip",
        region: "Shqipëria",
        english_name: "Albanian",
    },
    McLocaleInfo {
        code: "mk_mk",
        name: "Македонски",
        region: "Северна Македонија",
        english_name: "Macedonian",
    },
    McLocaleInfo {
        code: "bs_ba",
        name: "Bosanski",
        region: "Bosna i Hercegovina",
        english_name: "Bosnian",
    },
    McLocaleInfo {
        code: "la_va",
        name: "Latina",
        region: "Civitas Vaticana",
        english_name: "Latin",
    },
    McLocaleInfo {
        code: "eo_uy",
        name: "Esperanto",
        region: "Mondo",
        english_name: "Esperanto",
    },
    McLocaleInfo {
        code: "tlh_aa",
        name: "tlhIngan Hol",
        region: "Qo'noS",
        english_name: "Klingon",
    },
    McLocaleInfo {
        code: "qya_aa",
        name: "Quenya",
        region: "Arda",
        english_name: "Elvish (Quenya)",
    },
    McLocaleInfo {
        code: "jbo_en",
        name: "la .lojban.",
        region: "terdi",
        english_name: "Lojban",
    },
    McLocaleInfo {
        code: "en_pt",
        name: "Pirate Speak",
        region: "The Seven Seas",
        english_name: "Pirate Speak",
    },
    McLocaleInfo {
        code: "en_ws",
        name: "Shakespearean",
        region: "Globe Theatre",
        english_name: "Shakespearean English",
    },
    McLocaleInfo {
        code: "en_7s",
        name: "70s English",
        region: "Discotheque",
        english_name: "70s English",
    },
    McLocaleInfo {
        code: "en_ud",
        name: "ɥsᴉꞁᵷuƎ",
        region: "uʍop ǝpᴉsdՈ",
        english_name: "Upside Down",
    },
    McLocaleInfo {
        code: "val_es",
        name: "Valencià",
        region: "Comunitat Valenciana",
        english_name: "Valencian",
    },
    McLocaleInfo {
        code: "ast_es",
        name: "Asturianu",
        region: "Asturies",
        english_name: "Asturian",
    },
    McLocaleInfo {
        code: "szl_pl",
        name: "Ślōnskŏ gŏdka",
        region: "Ślōnsk",
        english_name: "Silesian",
    },
    McLocaleInfo {
        code: "csb_pl",
        name: "Kaszëbsczi",
        region: "Kaszëbë",
        english_name: "Kashubian",
    },
];

/// Looks up locale information by Minecraft locale code (case-insensitive, e.g. `"uk_ua"`).
pub fn find_mc_locale(code: &str) -> Option<&'static McLocaleInfo> {
    let lower = code.trim().to_ascii_lowercase();
    ALL_MC_LOCALES.iter().find(|loc| loc.code == lower)
}

/// Searches the locale catalog by query substring across code, name, region, and english_name.
pub fn search_mc_locales(query: &str) -> Vec<&'static McLocaleInfo> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return ALL_MC_LOCALES.iter().collect();
    }
    ALL_MC_LOCALES
        .iter()
        .filter(|loc| {
            loc.code.contains(&q)
                || loc.name.to_lowercase().contains(&q)
                || loc.region.to_lowercase().contains(&q)
                || loc.english_name.to_lowercase().contains(&q)
        })
        .collect()
}

/// Automatically maps a launcher language code (e.g. `"uk_UA"`, `"ru_RU"`, `"en_US"`)
/// to the corresponding Minecraft game locale code (`"uk_ua"`, `"ru_ru"`, `"en_us"`).
pub fn launcher_locale_to_mc_locale(launcher_locale: &str) -> &'static str {
    match launcher_locale.trim() {
        "uk_UA" | "uk_ua" | "uk" => "uk_ua",
        "ru_RU" | "ru_ru" | "ru" => "ru_ru",
        "pl_PL" | "pl_pl" | "pl" => "pl_pl",
        "de_DE" | "de_de" | "de" => "de_de",
        "fr_FR" | "fr_fr" | "fr" => "fr_fr",
        "es_ES" | "es_es" | "es" => "es_es",
        "be_BY" | "be_by" | "be" => "be_by",
        "kk_KZ" | "kk_kz" | "kk" => "kk_kz",
        "zh_CN" | "zh_cn" | "zh" => "zh_cn",
        "ja_JP" | "ja_jp" | "ja" => "ja_jp",
        _ => "en_us",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_mc_locales_contains_80_plus_locales() {
        assert!(
            ALL_MC_LOCALES.len() >= 80,
            "must contain 80+ locales, got {}",
            ALL_MC_LOCALES.len()
        );
        assert!(find_mc_locale("uk_ua").is_some());
        assert!(find_mc_locale("ru_ru").is_some());
        assert!(find_mc_locale("en_us").is_some());
        assert!(find_mc_locale("pl_pl").is_some());
    }

    #[test]
    fn search_locales_finds_ukraine_and_russia() {
        let uk = search_mc_locales("укра");
        assert!(!uk.is_empty());
        assert_eq!(uk[0].code, "uk_ua");

        let ru = search_mc_locales("рус");
        assert!(!ru.is_empty());
        assert_eq!(ru[0].code, "ru_ru");
    }

    #[test]
    fn mapping_launcher_to_mc_locale_works_accurately() {
        assert_eq!(launcher_locale_to_mc_locale("uk_UA"), "uk_ua");
        assert_eq!(launcher_locale_to_mc_locale("ru_RU"), "ru_ru");
        assert_eq!(launcher_locale_to_mc_locale("pl_PL"), "pl_pl");
        assert_eq!(launcher_locale_to_mc_locale("en_US"), "en_us");
    }
}
