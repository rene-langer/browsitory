// CI proof: frontend-only change (throwaway, do not merge).
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { UpdateBanner } from './components/UpdateBanner'
import { installGlobalErrorLogging } from './lib/logger'
import { tauriRepoClient } from './ipc/tauriRepoClient'
import { thirdPartyNoticesLoader, thirdPartyNoticesUrl } from './lib/thirdPartyNotices'

installGlobalErrorLogging(tauriRepoClient)

const loadThirdPartyNotices = thirdPartyNoticesLoader(thirdPartyNoticesUrl(null, document.baseURI))

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App client={tauriRepoClient} updateBanner={<UpdateBanner />} loadThirdPartyNotices={loadThirdPartyNotices} />
  </StrictMode>,
)
