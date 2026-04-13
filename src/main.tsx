import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { ToastProvider } from './components/ToastProvider'

createRoot(document.getElementById('root')!, {
  onRecoverableError(error, errorInfo) {
    if (import.meta.env.DEV) {
      console.error('[NineClaw recoverable render error]', error, errorInfo.componentStack)
    }
  },
}).render(
  <StrictMode>
    <ToastProvider>
      <App />
    </ToastProvider>
  </StrictMode>,
)
