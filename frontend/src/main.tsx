import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { UpdateBanner } from './components/UpdateBanner'
import { installGlobalErrorLogging } from './lib/logger'
import { loadKeymap } from './lib/keymap'
import { tauriRepoClient } from './ipc/tauriRepoClient'
import { thirdPartyNoticesLoader, thirdPartyNoticesUrl } from './lib/thirdPartyNotices'
import { setHighlightTransport } from './lib/highlight'
import { inProcessTransport } from './lib/highlightInProcess'
import { workerTransport } from './lib/highlightWorkerProtocol'

installGlobalErrorLogging(tauriRepoClient)
// The user's keyboard shortcuts (#898), before anything shows or binds a key.
loadKeymap()
// Syntax highlighting runs in a Web Worker (#843); on the page in chunks if it can't start.
setHighlightTransport(
  workerTransport(
    () => new Worker(new URL('./lib/highlight.worker.ts', import.meta.url), { type: 'module' }),
    inProcessTransport(),
  ),
)

const loadThirdPartyNotices = thirdPartyNoticesLoader(thirdPartyNoticesUrl(null, document.baseURI))

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App client={tauriRepoClient} updateBanner={<UpdateBanner />} loadThirdPartyNotices={loadThirdPartyNotices} />
  </StrictMode>,
)
