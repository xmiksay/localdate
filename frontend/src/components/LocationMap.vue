<script setup lang="ts">
// Loaded only through defineAsyncComponent, so leaflet and its CSS stay in their own chunk.
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import L from 'leaflet'
import 'leaflet/dist/leaflet.css'
import type { Area } from '@/api/types'
import type { Coords } from '@/utils/geo'
import { mapLabel } from '@/utils/mapLabel'

const props = defineProps<{ at: Coords | null; areas: Area[]; label: string }>()
const emit = defineEmits<{ pick: [at: Coords] }>()

const TILES = 'https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png'
const ATTRIBUTION =
  '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors'
/** Prague, until a position is set. */
const FALLBACK: L.LatLngTuple = [50.0875, 14.4213]
const PLUM = '#4a2c5a'
// A divIcon: leaflet's default marker resolves its PNGs relative to the CSS, which the bundler breaks.
const ICON = L.divIcon({
  className: '',
  html: '<span style="display:block;width:24px;height:24px;border-radius:9999px;background:#e2552d;border:3px solid #fffaf3;box-shadow:0 1px 4px rgb(0 0 0 / .4)"></span>',
  iconSize: [24, 24],
  iconAnchor: [12, 12],
})

const el = ref<HTMLDivElement | null>(null)
let map: L.Map | null = null
let marker: L.Marker | null = null
let areaLayer: L.LayerGroup | null = null

const toCoords = (ll: L.LatLng): Coords => ({ lat: ll.lat, lon: ll.lng })

function placeMarker(at: Coords | null) {
  if (!map) return
  if (!at) {
    marker?.remove()
    marker = null
    return
  }
  if (marker) return void marker.setLatLng([at.lat, at.lon])
  const m = L.marker([at.lat, at.lon], { icon: ICON, draggable: true, keyboard: false })
  m.on('dragend', () => emit('pick', toCoords(m.getLatLng())))
  marker = m.addTo(map)
}

function drawAreas(areas: Area[]) {
  areaLayer?.clearLayers()
  for (const a of areas) {
    L.circle([a.lat, a.lon], { radius: a.radius_m, color: PLUM, weight: 2, fillOpacity: 0.08 })
      .bindTooltip(mapLabel(a.name))
      .addTo(areaLayer as L.LayerGroup)
  }
}

onMounted(() => {
  if (!el.value) return
  const at = props.at
  map = L.map(el.value).setView(at ? [at.lat, at.lon] : FALLBACK, at ? 15 : 13)
  L.tileLayer(TILES, { attribution: ATTRIBUTION, maxZoom: 19 }).addTo(map)
  areaLayer = L.layerGroup().addTo(map)
  map.on('click', (e: L.LeafletMouseEvent) => emit('pick', toCoords(e.latlng)))
  placeMarker(at)
  drawAreas(props.areas)
})

watch(
  () => props.at,
  (at) => {
    placeMarker(at)
    if (at) map?.panTo([at.lat, at.lon])
  },
)
watch(() => props.areas, drawAreas)

onBeforeUnmount(() => {
  map?.remove()
  map = null
  marker = null
  areaLayer = null
})
</script>

<template>
  <!-- `isolate`: leaflet's panes use z-indexes up to 1000, which must not cover the nav or dialogs. -->
  <div
    ref="el"
    role="application"
    :aria-label="label"
    class="isolate h-72 w-full overflow-hidden rounded-2xl border-2 border-line"
  />
</template>
