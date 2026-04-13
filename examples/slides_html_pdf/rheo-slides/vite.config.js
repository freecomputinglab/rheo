import { defineConfig } from "vite";
import replace from "rollup-plugin-replace";
import { viteStaticCopy } from "vite-plugin-static-copy";

export default defineConfig({
  plugins: [
    // Hack to allow Tippy.js to run in the browser.
    // See: https://atomiks.github.io/tippyjs/v5/faq/#i-m-getting-uncaught-referenceerror-process-is-not-defined
    replace({
      'process.env.NODE_ENV': JSON.stringify('development'),
    }),

    viteStaticCopy({
      targets: [
        { src: "src/rheo-slides.typ", dest: "", rename: { stripBase: 1 } }
      ]
    })
  ],
  build: {
    lib: {
      entry: "src/lib.ts",
      formats: ["iife"],
      name: "RheoSlidesComponent",
      fileName: () => "rheo-slides.js",
    },
    outDir: "dist",
  },
});
