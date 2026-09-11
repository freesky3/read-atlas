export type LatinFontId = "default" | "consolas" | "times" | "inter" | "latex";
export type ChineseFontId = "default" | "pingfang" | "kaiti" | "songti";

export type FontOption<T extends string> = {
  id: T;
  css: string;
};

export const LATIN_FONT_OPTIONS: FontOption<LatinFontId>[] = [
  {
    id: "default",
    css: '-apple-system, BlinkMacSystemFont, "Inter", "Segoe UI", Roboto',
  },
  {
    id: "consolas",
    css: 'Consolas, "Fira Code", "Source Code Pro", Menlo, monospace',
  },
  {
    id: "times",
    css: '"Times New Roman", Times, Georgia, "TeX Gyre Termes", serif',
  },
  {
    id: "inter",
    css: '"Inter", -apple-system, "Segoe UI", sans-serif',
  },
  {
    id: "latex",
    css: '"Computer Modern Roman", "CMU Serif", "Latin Modern Roman", Georgia, serif',
  },
];

export const CHINESE_FONT_OPTIONS: FontOption<ChineseFontId>[] = [
  {
    id: "default",
    css: '"PingFang SC", "Microsoft YaHei", "Hiragino Sans GB", "Segoe UI"',
  },
  {
    id: "pingfang",
    css: '"PingFang SC", "Heiti SC", "Microsoft YaHei", "Noto Sans SC", sans-serif',
  },
  {
    id: "kaiti",
    css: '"LXGW WenKai", "Kaiti SC", KaiTi, "STKaiti", serif',
  },
  {
    id: "songti",
    css: '"Songti SC", SimSun, "STSong", "Source Han Serif SC", serif',
  },
];

export const LATIN_FONT_STORAGE_KEY = "read-desktop.latinFont";
export const CHINESE_FONT_STORAGE_KEY = "read-desktop.chineseFont";

export type FontPreferences = {
  latin: LatinFontId;
  chinese: ChineseFontId;
};

export function getSavedFontPreferences(): FontPreferences {
  let latin: LatinFontId = "default";
  let chinese: ChineseFontId = "default";

  try {
    const savedLatin = localStorage.getItem(LATIN_FONT_STORAGE_KEY);
    if (
      savedLatin === "default" ||
      savedLatin === "consolas" ||
      savedLatin === "times" ||
      savedLatin === "inter" ||
      savedLatin === "latex"
    ) {
      latin = savedLatin;
    }

    const savedChinese = localStorage.getItem(CHINESE_FONT_STORAGE_KEY);
    if (
      savedChinese === "default" ||
      savedChinese === "pingfang" ||
      savedChinese === "kaiti" ||
      savedChinese === "songti"
    ) {
      chinese = savedChinese;
    }
  } catch {
    // Ignore in tests or SSR
  }

  return { latin, chinese };
}

export function buildCombinedFontFamily(
  latin: LatinFontId,
  chinese: ChineseFontId,
): string {
  const latinOpt =
    LATIN_FONT_OPTIONS.find((opt) => opt.id === latin) ?? LATIN_FONT_OPTIONS[0];
  const chineseOpt =
    CHINESE_FONT_OPTIONS.find((opt) => opt.id === chinese) ??
    CHINESE_FONT_OPTIONS[0];

  return `${latinOpt.css}, ${chineseOpt.css}, sans-serif`;
}

export function applyFontPreferences(prefs: FontPreferences) {
  const combinedCss = buildCombinedFontFamily(prefs.latin, prefs.chinese);

  if (typeof document !== "undefined") {
    document.documentElement.style.setProperty("--app-font", combinedCss);
    document.documentElement.dataset.latinFont = prefs.latin;
    document.documentElement.dataset.chineseFont = prefs.chinese;
    if (document.body) {
      document.body.style.fontFamily = combinedCss;
    }
  }

  try {
    localStorage.setItem(LATIN_FONT_STORAGE_KEY, prefs.latin);
    localStorage.setItem(CHINESE_FONT_STORAGE_KEY, prefs.chinese);
  } catch {
    // Ignore
  }
}
