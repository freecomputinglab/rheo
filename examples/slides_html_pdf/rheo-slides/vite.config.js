import { defineConfig } from "vite";
import { viteStaticCopy } from "vite-plugin-static-copy";

export default defineConfig({
  plugins: [
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
      name: "RheoSlides",
      fileName: () => "rheo-slides.js",
    },
    outDir: "dist",
  },
});
