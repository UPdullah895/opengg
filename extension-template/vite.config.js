import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import cssInjectedByJs from 'vite-plugin-css-injected-by-js'

// Builds src/index.ts into a single, self-contained IIFE bundle at
// dist/index.iife.js. OpenGG fetches and evaluates that one file, so CSS from
// scoped <style> blocks is injected into the page at runtime by JS (rather than
// emitted as a separate .css file the loader would never request).
//
// OpenGG provides Vue at runtime on `window.Vue`, so Vue is marked external and
// mapped to the `Vue` global — it is NOT bundled. The bundle self-registers
// `window.__ext_<id>` when it runs, so the library `name` below is only cosmetic.
export default defineConfig({
  plugins: [vue(), cssInjectedByJs()],
  build: {
    lib: {
      entry: 'src/index.ts',
      name: 'OpenGGExtension',
      formats: ['iife'],
      fileName: () => 'index.iife.js',
    },
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      external: ['vue'],
      output: { globals: { vue: 'Vue' } },
    },
  },
})
