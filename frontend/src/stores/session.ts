import { useAdminStore } from './admin'
import { useAdminUsersStore } from './adminUsers'
import { useAreasStore } from './areas'
import { useIdentitiesStore } from './identities'
import { useMatchesStore } from './matches'
import { useMeStore } from './me'
import { useNearbyStore } from './nearby'
import { usePinnedLocationStore } from './pinnedLocation'
import { usePushStore } from './push'
import { useSafetyStore } from './safety'
import { useWindowStore } from './window'

/** Whenever the acting user changes (logout, auth loss, impersonation start / end). */
export function resetUserStores() {
  useMeStore().reset()
  useWindowStore().reset()
  useNearbyStore().reset()
  useMatchesStore().reset()
  useAdminStore().reset()
  useAdminUsersStore().reset()
  useAreasStore().reset()
  useIdentitiesStore().reset()
  usePushStore().reset()
  useSafetyStore().reset()
  usePinnedLocationStore().reset()
}
