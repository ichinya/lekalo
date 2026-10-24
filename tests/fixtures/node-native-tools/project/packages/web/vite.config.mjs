export default {resolve:{preserveSymlinks:true},cacheDir:'cache/vite',server:{host:'127.0.0.1'},build:{outDir:'out',minify:false,rollupOptions:{output:{entryFileNames:'assets/main.js',chunkFileNames:'assets/chunk.js',assetFileNames:'assets/resource[extname]'}}}};

