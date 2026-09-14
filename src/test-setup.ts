import '@testing-library/jest-dom/vitest';

Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: (query: string) => ({ matches: false, media: query, onchange: null, addListener: () => undefined, removeListener: () => undefined, addEventListener: () => undefined, removeEventListener: () => undefined, dispatchEvent: () => false }),
});

Object.assign(navigator, { clipboard: { writeText: async () => undefined } });

// jsdom reports `en-US`, which would silently flip every suite to the English
// dictionary. Pin the system locale so language is something a test opts into.
Object.defineProperty(navigator, 'language', { value: 'zh-CN', configurable: true });

if (!URL.createObjectURL) URL.createObjectURL = () => 'blob:agenthub-test';
if (!URL.revokeObjectURL) URL.revokeObjectURL = () => undefined;
