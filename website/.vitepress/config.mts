import { defineConfig } from 'vitepress';

const base = process.env.BASE_PATH ?? '/';

export default defineConfig({
  title: 'ws-mcp',
  description:
    'ws-mcp is an unofficial MCP server and CLI that lets an agent invest on Wealthsimple, within the limits you set.',
  base,
  cleanUrls: true,
  appearance: false,
  srcDir: 'pages',
  vite: {
    publicDir: 'assets',
    build: {
      chunkSizeWarningLimit: 1024,
    },
  },
  head: [
    ['link', { rel: 'preconnect', href: 'https://fonts.googleapis.com' }],
    ['link', { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: '' }],
    [
      'link',
      {
        rel: 'stylesheet',
        href: 'https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600&family=Playfair+Display:wght@400;500;600&display=swap',
      },
    ],
  ],
});
