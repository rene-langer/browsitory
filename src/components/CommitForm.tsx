import { useState, type FormEvent } from 'react'

interface CommitFormProps {
  disabled: boolean
  onCommit: (message: string, author: { name: string; email: string }) => void
}

const NAME_KEY = 'browsitory:author-name'
const EMAIL_KEY = 'browsitory:author-email'

export default function CommitForm({ disabled, onCommit }: CommitFormProps) {
  const [message, setMessage] = useState('')
  const [name, setName] = useState(() => localStorage.getItem(NAME_KEY) ?? '')
  const [email, setEmail] = useState(() => localStorage.getItem(EMAIL_KEY) ?? '')

  const canSubmit =
    !disabled && message.trim().length > 0 && name.trim().length > 0 && email.trim().length > 0

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (!canSubmit) return
    localStorage.setItem(NAME_KEY, name)
    localStorage.setItem(EMAIL_KEY, email)
    onCommit(message.trim(), { name: name.trim(), email: email.trim() })
    setMessage('')
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-2 p-4 border-t border-border">
      <div className="grid grid-cols-2 gap-2">
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Author name"
          className="rounded-md border border-border bg-background text-foreground p-2 text-sm"
        />
        <input
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder="Author email"
          className="rounded-md border border-border bg-background text-foreground p-2 text-sm"
        />
      </div>
      <textarea
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        placeholder="Commit message"
        rows={3}
        className="w-full rounded-md border border-border bg-background text-foreground p-2 text-sm"
      />
      <button
        type="submit"
        disabled={!canSubmit}
        className="w-full px-4 py-2 bg-primary text-primary-foreground rounded-md hover:opacity-90 transition disabled:opacity-50 disabled:cursor-not-allowed"
      >
        Commit
      </button>
    </form>
  )
}
