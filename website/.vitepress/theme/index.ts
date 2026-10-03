import DefaultTheme from 'vitepress/theme';
import { h } from 'vue';
import type { Theme } from 'vitepress';
import HeroFine from './HeroFine.vue';
import HeroHeadline from './HeroHeadline.vue';
import HomeModel from './HomeModel.vue';
import './custom.css';

export default {
  extends: DefaultTheme,
  Layout: () => {
    return h(DefaultTheme.Layout, null, {
      'home-hero-info': () => h(HeroHeadline),
      'home-hero-actions-after': () => h(HeroFine),
      'home-hero-after': () => h(HomeModel),
    });
  },
} satisfies Theme;
