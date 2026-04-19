/// <reference types="vite/client" />

import Reveal from 'reveal.js';
import revealCss from 'reveal.js/dist/reveal.css?raw';
import themeCss from 'reveal.js/dist/theme/white.css?raw';
import myStyle from './style.css?raw'; // Import your CSS as string

const style = document.createElement('style');
style.textContent = revealCss + themeCss;
document.head.appendChild(style);

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', () => Reveal.initialize({ hash: true }));
} else {
  Reveal.initialize({ hash: true });
}
