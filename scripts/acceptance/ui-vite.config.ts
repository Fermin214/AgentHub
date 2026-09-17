import { mergeConfig } from 'vite';
import base from '../../vite.config';

// Acceptance evidence can contain archived HTML/source checkouts. Do not crawl
// those as application entry points during a native development run.
export default mergeConfig(base, { optimizeDeps: { entries: ['index.html'] } });
