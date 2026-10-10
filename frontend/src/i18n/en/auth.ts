import type cs from '../cs/auth'
import type { Messages } from '../messages'

const en: Messages<typeof cs> = {
  auth: {
    username: 'Username',
    usernameHint: '1–64 characters; spaces, accents and emoji are fine. Login ignores letter case.',
    password: 'Password',
    passwordHint: '7–128 characters.',
    loginTitle: 'Welcome back',
    loginSubmit: 'Log in',
    loginSwitch: "Don't have an account?",
    registerTitle: 'Create your account',
    registerSubmit: 'Sign up',
    registerSwitch: 'Already have an account?',
    invalidUsername: 'A username must be 1 to 64 characters, at least one visible, without control or invisible characters (such as a line break).',
    invalidPassword: 'Password must be 7 to 128 characters.',
    suspendedTitle: 'Account suspended',
    suspendedBody:
      'Your account was suspended for breaking the rules. You can no longer log in or use the app.',
    email: 'Email',
    emailOption: 'Log in with email',
    emailSubmit: 'Send link',
    invalidEmail: 'Enter a valid email address.',
    emailSentTitle: 'Check your email',
    emailSentBody: 'We sent a link to {email}. It is valid for 15 minutes.',
    emailOther: 'Use another address',
    emailSignupTitle: 'Finish signing up',
    emailSignupIntro: 'Pick a username for {email}.',
    emailLinkInvalid: 'This link is invalid or has expired. Request a new one.',
    toLogin: 'Back to login',
    emailLoginAs: 'Log in as {username}',
  },
  identities: {
    title: 'Linked accounts',
    provider: { email: 'Email' },
    verified: 'Verified {date}',
    remove: 'Remove',
    empty: 'No linked accounts yet.',
    noPassword: 'This account has no password — you log in with a linked account.',
    addEmail: 'Add email',
    linked: 'Email {email} is now linked.',
    linkTitle: 'Link {email} to the account {username}?',
    linkSubmit: 'Link',
    linkInvalid: 'The link has expired or belongs to another account.',
    toSettings: 'Back to settings',
  },
  password: {
    forgotLink: 'Forgot your password?',
    forgotTitle: 'Forgotten password',
    forgotIntro:
      "Enter your username or email. If the account has a linked email or Telegram, we'll send a link to set a new password there. Telegram messages go out only when you enter the username.",
    login: 'Username or email',
    invalidLogin: 'Enter a valid username or email address.',
    forgotSubmit: 'Send link',
    forgotIntroTelegram:
      "Enter your username. If the account has a linked Telegram, we'll send you a link there to set a new password.",
    forgotSentBodyTelegram:
      "If the account has a linked Telegram, we've sent you a link there. It is valid for 15 minutes. An account without Telegram cannot reset its password.",
    forgotSentBody:
      "If the account has a linked email or Telegram, we've sent a link there. It is valid for 15 minutes. An account with neither cannot reset its password.",
    resetTitle: 'Set a new password for {username}',
    resetSubmit: 'Save password',
    resetDone:
      'Your password is changed and every device is logged out. Log in with the new password.',
    title: 'Password',
    current: 'Current password',
    new: 'New password',
    confirm: 'New password again',
    mismatch: "The passwords don't match.",
    wrongCurrent: 'The current password is wrong.',
    currentNeeded: 'This account has a password by now. Enter it as the current one.',
    noPasswordIntro: 'This account has no password yet. Set one to also log in with your username.',
    changeSubmit: 'Change password',
    setSubmit: 'Set password',
    saved: 'Password saved. Your other devices were logged out.',
  },
  settings: {
    title: 'Settings',
    filter: 'Filter',
    language: 'Language',
    account: 'Account',
    loggedInAs: 'Logged in as {username}',
    admin: 'Moderation and areas',
  },
}

export default en
