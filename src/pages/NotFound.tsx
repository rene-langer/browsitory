import { Link } from 'react-router-dom'

export default function NotFound() {
  return (
    <div className="p-8 text-center">
      <h1 className="text-4xl font-bold text-foreground mb-4">404</h1>
      <p className="text-lg text-muted-foreground mb-6">Page not found</p>
      <Link to="/" className="text-primary hover:underline">
        Go back to dashboard
      </Link>
    </div>
  )
}
