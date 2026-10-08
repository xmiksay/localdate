// Source of truth for every message; one module per domain keeps each file small.
import core from './core'
import auth from './auth'
import profile from './profile'
import discovery from './discovery'
import moderation from './moderation'
import notifications from './notifications'

export default { ...core, ...auth, ...profile, ...discovery, ...moderation, ...notifications }
