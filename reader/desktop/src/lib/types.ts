export type Title = {
  titleId: number;
  name: string;
  author: string;
  portraitImageUrl: string;
  landscapeImageUrl: string;
  viewCount: number;
  language: number;
  titleUpdateStatus: number;
  favoriteImageUrl: string;
};

export type Chapter = {
  titleId: number;
  chapterId: number;
  name: string;
  subTitle: string;
  thumbnailUrl: string;
  startTimeStamp: number;
  endTimeStamp: number;
  alreadyViewed: boolean;
  isVerticalOnly: boolean;
  isHorizontalOnly: boolean;
  chapterTicketEndtime: number;
  viewedForFree: boolean;
  isUpdated: boolean;
  chapterType: number;
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
  chapters: Chapter[];  // full chapter list of the parent title (field 3)
  titleName: string;
  chapterName: string;
  isVerticalOnly: boolean;
  titleId: number;
  startFromRight: boolean;
  titleLanguage: string;
};

// What /title_list/bookmark actually returns. Java method is named
// getFavoriteTitles() and the endpoint URL says "bookmark", but the
// server response is a SubscribedTitlesView (oneof field 7) — confirmed
// by wire-probing the live API. titles is a flat list.
export type SubscribedTitlesView = {
  titles: Title[];
};

// Kept for documentation; the bookmark endpoint does NOT return this.
export type FavoriteTitleGroup = {
  language: number;
  titles: Title[];
};

export type FavoriteTitlesView = {
  favoriteTitles: FavoriteTitleGroup[];
};

export type TitleList = {
  listName: string;
  featuredTitles: Title[];
};

export type SearchContents = {
  titleList?: TitleList;
};

export type SearchView = {
  contents: SearchContents[];
};
