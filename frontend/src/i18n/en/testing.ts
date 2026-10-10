import type cs from '../cs/testing'
import type { Messages } from '../messages'

const en: Messages<typeof cs> = {
  impersonation: {
    actAs: 'Act as',
    actAsLabel: 'Act as @{username}',
    banner: 'Acting as {username}',
    stop: 'Stop',
    expired: 'Acting as the user expired — you are back as admin',
    banned: 'The user was banned — you are back as admin',
    dismiss: 'Got it',
    settingsNote:
      'While acting as another user, the password, linked accounts, notifications and blocks cannot be changed and the account cannot be deleted.',
  },
  position: {
    title: 'Location',
    intro:
      'Location is not shared automatically now. Set it from this device, or tap the map or drag the marker.',
    useDevice: "Use this device's location",
    duration: 'Length of new visibility',
    startsWindow: 'Visibility is off: setting a location turns it on at that point.',
    current: 'Set to: {lat}, {lon}',
    none: 'No location set yet.',
    saving: 'Saving location…',
    mapLabel: 'Map to pick a location',
  },
  adminUsers: {
    searchLabel: 'Search users',
    searchHint: 'Username or display name; leave empty for the newest accounts.',
    empty: 'Nobody found.',
    age: '{n} yrs',
  },
  testUsers: {
    intro:
      'Test accounts have no password; you drive them with "Act as". Other users see them like anyone else.',
    add: 'New test user',
    empty: 'No test users yet.',
    username: 'Username',
    usernameInvalid: '1 to 64 characters, at least one visible, no control characters.',
    gender: 'Gender',
    photo: 'Photo (optional)',
    photoHint: 'Without a photo a placeholder avatar is generated.',
    create: 'Create',
    delete: 'Delete',
    deleteTitle: 'Delete @{username}?',
    deleteBody:
      'The account and everything that belongs to it is deleted for good, like a deleted account.',
    photoFailed: 'Account @{username} was created, but the photo upload failed: {error}',
    impersonationOff: 'Acting as users is switched off on the server (ADMIN_IMPERSONATION).',
  },
  audit: {
    intro: 'The latest 200 entries of admins acting as users and the changes made meanwhile.',
    empty: 'No entries yet.',
    action: {
      impersonate: 'Acted as user',
      impersonated_request: 'Request as user',
    },
    deleted: 'deleted account',
  },
}

export default en
