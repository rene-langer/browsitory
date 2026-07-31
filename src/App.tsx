import { BrowserRouter, Routes, Route } from 'react-router-dom'
import Layout from '@components/Layout'
import Dashboard from '@pages/Dashboard'
import Repository from '@pages/Repository'
import NotFound from '@pages/NotFound'

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route index element={<Dashboard />} />
          <Route path="repo/:id" element={<Repository />} />
          <Route path="*" element={<NotFound />} />
        </Route>
      </Routes>
    </BrowserRouter>
  )
}

export default App
