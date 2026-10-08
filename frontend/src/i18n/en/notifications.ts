import type cs from '../cs/notifications'
import type { Messages } from '../messages'

const en: Messages<typeof cs> = {
  push: {
    title: 'Notifications',
    intro:
      "When the app isn't open, we'll let you know about waves, new matches and messages. Notifications never contain names or message text.",
    enable: 'Turn on notifications',
    disable: 'Turn off notifications on this device',
    enabledHere: 'Notifications are on for this device.',
    kinds: 'Notify me about',
    waves: 'Waves',
    matches: 'New matches',
    messages: 'Messages',
    denied:
      'Notifications are blocked in your browser. Allow them for this site in the browser settings.',
    unsupported: "This browser doesn't support notifications.",
    unavailable: 'Notifications are not available on this server.',
    iosInstall:
      'On iPhone and iPad, notifications only work in the app added to the home screen: in Safari tap Share → Add to Home Screen and open localdate from there.',
  },
}

export default en
