import { useState, type FormEvent } from 'react'
import { ArrowRight, LockKeyhole } from 'lucide-react'
import { Button, Callout, TextField } from '../ui'
import { gatewayApi } from '../../api/gateway'

export function SignInScreen({ onSignedIn }: { onSignedIn: () => void }) {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    setBusy(true)
    setError('')
    try {
      await gatewayApi.signIn(username, password)
      setPassword('')
      onSignedIn()
    } catch {
      setError('Sign-in failed. Check the username and password.')
    } finally {
      setBusy(false)
    }
  }

  return (
    <main className="signin">
      <form className="signin-card" onSubmit={submit}>
        <span className="brand-mark large"><LockKeyhole size={20} aria-hidden /></span>
        <h1>Sign in to Code Mode</h1>
        <p>Use the admin account you created with <code>codemode admin setup</code>.</p>
        {error && <Callout tone="danger">{error}</Callout>}
        <TextField label="Username" value={username} onChange={setUsername} autoFocus autoComplete="username" required />
        <TextField label="Password" type="password" value={password} onChange={setPassword} autoComplete="current-password" required />
        <Button type="submit" variant="primary" className="full" loading={busy} disabled={!username || !password}>
          Sign in <ArrowRight size={16} aria-hidden />
        </Button>
        <p className="signin-hint">Forgot the password? Run <code>codemode admin reset-password</code>.</p>
      </form>
    </main>
  )
}
