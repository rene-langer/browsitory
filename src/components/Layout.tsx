import { Outlet } from 'react-router-dom'

export default function Layout() {
  return (
    <div className="flex h-screen bg-background">
      {/* Sidebar */}
      <div className="w-64 border-r border-border bg-card">
        <div className="p-4">
          <h1 className="text-2xl font-bold text-foreground">Browsitory</h1>
        </div>
      </div>

      {/* Main content */}
      <div className="flex-1 flex flex-col">
        {/* Header */}
        <div className="border-b border-border bg-card p-4">
          <h2 className="text-lg font-semibold text-foreground">Welcome to Browsitory</h2>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-auto">
          <Outlet />
        </div>
      </div>
    </div>
  )
}
