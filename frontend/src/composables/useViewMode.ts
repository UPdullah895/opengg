import { ref } from 'vue'

/** Module-level singleton — shared between ClipsPage and HomePage popovers. */
export const viewMode = ref<'grid' | 'list'>('grid')
