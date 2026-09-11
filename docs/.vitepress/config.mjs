import { defineSkipprDocs } from '@skippr/vitepress-theme'

export default defineSkipprDocs({
  name: 'Skippr ReAct',
  hostname: 'react.skippr.io',
  description: 'Generic ReAct runtime used by Skippr Data Engineer.',
  nav: [{"text": "Home", "link": "/"}],
  sidebar: {"/": [{"text": "ReAct", "link": "/"}]},
})
