// Sign in with an OAuth / OpenID Connect provider (#15)
import type cs from '../cs/oauth'
import type { Messages } from '../messages'

const en: Messages<typeof cs> = {
  oauth: {
    provider: { google: 'Google', telegram: 'Telegram', facebook: 'Facebook' },
    from: { google: 'from Google', telegram: 'from Telegram', facebook: 'from Facebook' },
    importPhoto: 'Import my profile picture {from}',
    importPhotoNew: 'For a new account, import my profile picture {from}',
    photo: {
      imported: 'Your profile picture was added to your photos.',
      pending: 'Your profile picture will be added once you pick a username.',
      full: 'Your profile picture was not added: you already have the maximum number of photos.',
      none: 'The account has no profile picture, so there was nothing to import.',
      failed: 'Your profile picture could not be imported. You can upload it yourself.',
    },
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
      identity_mismatch:
        'A different account of this provider is linked to yours. Sign in there with the linked one and try again.',
      unauthorized: 'Your session expired. Log in and try again.',
      rate_limited: 'Too many attempts. Wait a moment and try again.',
      internal: 'Something went wrong on our side. Please try again later.',
      unknown: 'The sign-in failed.',
    },
  },
}

export default en
