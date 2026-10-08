import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import { VitePWA } from 'vite-plugin-pwa'

const backend = 'http://127.0.0.1:3000'

export default defineConfig({
  plugins: [
    vue(),
    tailwindcss(),
    VitePWA({
      registerType: 'autoUpdate',
      // Own worker for push/notificationclick; it keeps the precache + SPA fallback (src/sw/sw.ts).
      strategies: 'injectManifest',
      srcDir: 'src/sw',
      filename: 'sw.ts',
      // PNGs come from scripts/icons.sh (`make icons`); notifications need raster images.
      includeAssets: ['icon.svg', 'icon-192.png', 'icon-512.png', 'badge-96.png'],
      manifest: {
        name: 'localdate',
        short_name: 'localdate',
        description: 'Meet people nearby, right now.',
        theme_color: '#e2552d',
        background_color: '#fbf4ea',
        display: 'standalone',
        start_url: '/',
        icons: [
          { src: 'icon.svg', sizes: 'any', type: 'image/svg+xml', purpose: 'any maskable' },
          { src: 'icon-192.png', sizes: '192x192', type: 'image/png', purpose: 'any maskable' },
          { src: 'icon-512.png', sizes: '512x512', type: 'image/png', purpose: 'any maskable' },
        ],
      },
    }),
  ],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  server: {
    proxy: {
      '/api': { target: backend, ws: true },
      '/media': { target: backend },
    },
  },
  test: { environment: 'jsdom', globals: true },
})
