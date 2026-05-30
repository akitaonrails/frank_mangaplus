// TS mirror of the proto types the Tauri commands deserialize. The Rust
// side (api/src/lib.rs::proto) is the ground truth — these types only
// include the fields the UI actually reads. Anything else the server
// sends still arrives on the wire; it's just elided from the TS model.

export type Title = {
  titleId: number;
  name: string;
  author: string;
  portraitImageUrl: string;
  language: number;
};

export type Chapter = {
  titleId: number;
  chapterId: number;
  name: string;
  subTitle: string;
  // Per-chapter thumbnail. Not currently rendered in the chapter list but
  // wired through so we can light it up without another type change.
  thumbnailUrl: string;
  isUpdated: boolean;
};

export type TitleDetailView = {
  title?: Title;
  titleImageUrl: string;
  overview: string;
  backgroundImageUrl: string;
  isSubscribed: boolean;
  chapterListGroup?: {
    chapterNumbers: string;
    firstChapterList: Chapter[];
    midChapterList: Chapter[];
    lastChapterList: Chapter[];
  };
  chapterListV2: Chapter[];
};

export type MangaPage = {
  imageUrl: string;
  width: number;
  height: number;
  type: number;
  encryptionKey: string;
};

export type Page = {
  data?: { mangaPage?: MangaPage };
};

export type MangaViewer = {
  pages: Page[];
  chapterId: number;
  // Full chapter list of the parent title (proto field 3). Used by the
  // reader to compute next/previous chapter for auto-advance.
  chapters: Chapter[];
  titleName: string;
  chapterName: string;
  isVerticalOnly: boolean;
  titleId: number;
  startFromRight: boolean;
  titleLanguage: string;
};

// /title_list/bookmark returns this despite the endpoint URL saying
// "bookmark" and the Java method being named getFavoriteTitles. Confirmed
// by wire-probing the live API.
export type SubscribedTitlesView = {
  titles: Title[];
};

export type SearchView = {
  contents: {
    titleList?: {
      featuredTitles: Title[];
    };
  }[];
};
