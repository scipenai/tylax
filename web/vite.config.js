import { defineConfig } from 'vite';

export default defineConfig({
    root: '.',
    base: './',
    build: {
        outDir: 'dist',
        emptyOutDir: true,
        target: 'esnext',
        minify: 'esbuild',
    },
    server: {
        port: 3000,
        open: true,
    },
    optimizeDeps: {
        exclude: ['./src/pkg'],
    },
});

