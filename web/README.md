# Aria Web

A PWA served by one Cloudflare Worker. The Worker does the search and audio lookups that browsers cannot do themselves (CORS, encrypted JioSaavn links, YouTube stream URLs) and passes the audio through.

- `src/saavn.ts` JioSaavn search, trending and stream URLs (DES decryption of the media token)
- `src/youtube.ts` YouTube Music search and AAC stream lookup (innertube, no yt-dlp)
- `src/index.ts` `/api/search`, `/api/trending`, `/api/audio` (range requests, bounded for YouTube)
- `public/` the installable app: manifest, service worker, UI

```
npm install
npm test
npm run dev      # http://localhost:8787
npm run deploy   # needs `npx wrangler login` once
```

YouTube may answer "Sign in to confirm you are not a bot" for some videos or IPs. JioSaavn does not have that problem.
