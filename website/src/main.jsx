import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.jsx'
import { scheduleLocalTimeTheme } from './theme/localTheme.js'

const stopLocalTimeThemeScheduler = scheduleLocalTimeTheme()

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    stopLocalTimeThemeScheduler?.()
  })
}

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
