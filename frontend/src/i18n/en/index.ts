import type cs from '../cs'
import type { Messages } from '../messages'
import core from './core'
import auth from './auth'
import profile from './profile'
import discovery from './discovery'
import moderation from './moderation'
import notifications from './notifications'
import oauth from './oauth'

// Typed against the whole cs tree as well, so a missing domain module fails the build.
const en: Messages<typeof cs> = {
  ...core,
  ...auth,
  ...profile,
  ...discovery,
  ...moderation,
  ...notifications,
  ...oauth,
}

export default en
