import { translations } from './translations';

class I18nManager {
  // Reactive state using Svelte 5 runes
  currentLang = $state<'zh' | 'en'>('zh');

  constructor() {
    if (typeof window !== 'undefined') {
      const stored = localStorage.getItem('cab_lang');
      if (stored === 'zh' || stored === 'en') {
        this.currentLang = stored;
      }
    }
  }

  setLang(l: 'zh' | 'en') {
    this.currentLang = l;
    if (typeof window !== 'undefined') {
      localStorage.setItem('cab_lang', l);
    }
  }

  t(key: string): string {
    const dict = translations[this.currentLang];
    const parts = key.split('.');
    let current: any = dict;
    
    for (const part of parts) {
      if (current && typeof current === 'object' && part in current) {
        current = current[part];
      } else {
        return key;
      }
    }
    
    return typeof current === 'string' ? current : key;
  }
}

export const i18n = new I18nManager();
