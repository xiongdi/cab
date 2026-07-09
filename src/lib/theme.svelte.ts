import { browser } from '$app/environment';

export type Theme = 'light' | 'dark' | 'system';

class ThemeManager {
  current = $state<Theme>('system');

  constructor() {
    if (browser) {
      const saved = localStorage.getItem('cab-theme') as Theme;
      this.current = saved || 'system';
      this.apply();

      // 监听系统 prefers-color-scheme 变化
      window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
        if (this.current === 'system') {
          this.apply();
        }
      });
    }
  }

  set(theme: Theme) {
    this.current = theme;
    if (browser) {
      localStorage.setItem('cab-theme', theme);
      this.apply();
    }
  }

  apply() {
    if (!browser) return;
    let effective: 'light' | 'dark' = 'dark';
    if (this.current === 'system') {
      effective = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    } else {
      effective = this.current;
    }
    document.documentElement.setAttribute('data-theme', effective);
  }
}

export const themeManager = new ThemeManager();
