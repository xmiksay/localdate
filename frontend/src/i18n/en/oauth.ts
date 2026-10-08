// Sign in with an OAuth / OpenID Connect provider (#15)
import type cs from '../cs/oauth'
import type { Messages } from '../messages'

const en: Messages<typeof cs> = {
  oauth: {
    provider: { google: 'Google' },
    continue: 'Continue with {provider}',
    link: 'Link {provider}',
    linked: 'Your {provider} account is linked.',
    signupTitle: 'Finish signing up',
    signupIntro: 'Your account is verified. Pick a username.',
    invalid: 'The sign-in expired or was already used. Please try again.',
    error: {
      cancelled: 'The sign-in was cancelled.',
      invalid_state: 'The sign-in expired or was started in another browser. Please try again.',
      oauth_failed: 'The sign-in provider did not confirm it. Please try again.',
      provider_disabled: 'This sign-in method is not available right now.',
      banned: 'This account has been suspended.',
      identity_taken: 'This account is already linked to another user.',
      unauthorized: 'Your session expired. Log in and try again.',
      rate_limited: 'Too many attempts. Wait a moment and try again.',
      internal: 'Something went wrong on our side. Please try again later.',
      unknown: 'The sign-in failed.',
    },
  },
}

export default en
