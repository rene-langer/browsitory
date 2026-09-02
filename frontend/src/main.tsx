import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { UpdateBanner } from './components/UpdateBanner'
import { installGlobalErrorLogging } from './lib/logger'
import { tauriRepoClient } from './ipc/tauriRepoClient'

installGlobalErrorLogging()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App client={tauriRepoClient} updateBanner={<UpdateBanner />} />
  </StrictMode>,
)
