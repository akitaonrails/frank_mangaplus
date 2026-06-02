import { invoke } from '@tauri-apps/api/core';
import type { SubscribedTitlesView, TitleDetailView } from './types';

export function getTitleDetail(args: {
  titleId: number;
  lang: string;
  clang: string;
  countryCode: string;
}): Promise<TitleDetailView> {
  return invoke('get_title_detail', args) as Promise<TitleDetailView>;
}

export function getFavorites(): Promise<SubscribedTitlesView> {
  return invoke('get_favorites') as Promise<SubscribedTitlesView>;
}

export function addFavorite(titleId: number): Promise<void> {
  return invoke('add_favorite', { titleId }) as Promise<void>;
}

export function removeFavorite(titleId: number): Promise<void> {
  return invoke('remove_favorite', { titleId }) as Promise<void>;
}
