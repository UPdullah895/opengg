<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useReplayStore, type Clip } from '../stores/replay'
import { usePersistenceStore } from '../stores/persistence'
import ClipCard from '../components/ClipCard.vue'
import ClipEditor from '../components/ClipEditor.vue'
import AdvancedEditor from '../components/AdvancedEditor.vue'

const replay = useReplayStore()
const persist = usePersistenceStore()

const editorClip = ref<Clip | null>(null)
const editorMode = ref<'preview' | 'trim'>('preview')
const advancedClip = ref<Clip | null>(null)
const renameTarget = ref<Clip | null>(null)
const renameValue = ref('')
const toast = ref('')

function showToast(msg: string) { toast.value = msg; setTimeout(() => toast.value = '', 3500) }
function refreshClips() { replay.fetchClips(persist.state?.settings?.clipsFolder || '', true) }

const gridCols = computed(() => persist.state?.settings?.clipsPerRow || 4)

function onCardClick(clip: Clip) {
  if (replay.selectMode) { replay.toggleSelect(clip.id); return }
  if (persist.state?.settings?.defaultClickAction === 'editor') openAdvanced(clip)
  else openPreview(clip)
}

function openPreview(clip: Clip) { editorClip.value = clip; editorMode.value = 'preview' }
function openAdvanced(clip: Clip) { advancedClip.value = clip }

function startRename(clip: Clip) { renameTarget.value = clip; renameValue.value = clip.custom_name || clip.filename.replace(/\.[^.]+$/, '') }
async function confirmRename() {
  if (!renameTarget.value) return
  const n = renameValue.value.trim()
  replay.updateClipMeta(renameTarget.value.filepath, { custom_name: n })
  try { await invoke('set_clip_meta', { update: { filepath: renameTarget.value.filepath, custom_name: n, favorite: renameTarget.value.favorite } }) } catch {}
  renameTarget.value = null
}

async function deleteClip(clip: Clip) {
  if (!confirm(`Delete "${clip.custom_name || clip.filename}"?`)) return
  try { await invoke('delete_clip', { filepath: clip.filepath }); replay.removeClip(clip.filepath); showToast('Clip deleted') } catch (e) { showToast(`Error: ${e}`) }
}

async function deleteSelected() {
  const ids = Array.from(replay.selectedIds)
  const clips = replay.clips.filter(c => ids.includes(c.id))
  if (!confirm(`Delete ${clips.length} clip(s)?`)) return
  for (const c of clips) { try { await invoke('delete_clip', { filepath: c.filepath }); replay.removeClip(c.filepath) } catch {} }
  replay.clearSelection(); showToast(`${clips.length} clip(s) deleted`)
}

onMounted(async () => {
  if (!persist.loaded) await persist.load()
  replay.fetchStatus()
  replay.fetchClips(persist.state?.settings?.clipsFolder || '')
})
</script>

<template>
  <div class="page">
    <!-- Header -->
    <div class="header">
      <h1 class="title">Clips</h1>
      <div class="header-r">
        <span class="rec-dot" :class="{ active: replay.status !== 'idle' }"></span>
        <span class="rec-txt">{{ replay.status === 'idle' ? 'Idle' : replay.status === 'replay' ? `Replay · ${replay.replayDuration}s` : 'Recording' }}</span>
        <button class="ib" @click="replay.status==='idle'?replay.startReplay():replay.stopRecorder()">{{ replay.status==='idle'?'▶':'■' }}</button>
        <button class="ib" @click="replay.saveReplay()" :disabled="replay.status!=='replay'">💾</button>
        <button class="ib" @click="refreshClips()">↻</button>
      </div>
    </div>

    <!-- Controls -->
    <div class="ctrl-bar">
      <div class="search-wrap"><svg class="search-ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg><input v-model="replay.search" placeholder="Search clips..." class="search" /></div>
      <div class="ctrl-grp"><label>Sort</label><select v-model="replay.sortMode" class="sel"><option value="newest">Newest</option><option value="oldest">Oldest</option><option value="longest">Longest</option><option value="shortest">Shortest</option></select></div>
      <div class="ctrl-grp"><label>Game</label><select v-model="replay.filterGame" class="sel"><option v-for="g in replay.games" :key="g" :value="g">{{ g==='all'?'All Games':g }}</option></select></div>
      <button class="fav-btn" :class="{ active: replay.filterFav }" @click="replay.filterFav=!replay.filterFav">❤ {{ replay.favCount }}</button>
      <div class="ctrl-grp"><label>Cols</label><select v-model.number="persist.state.settings.clipsPerRow" class="sel sel-sm"><option :value="3">3</option><option :value="4">4</option><option :value="5">5</option></select></div>
    </div>

    <!--
      ★ FIX 3: The scroll container.
      - overflow-y: auto → user can scroll
      - grid-auto-rows: max-content → cards are their natural height
      - NO flex:1 or height constraints that would squish cards
    -->
    <div class="scroll-area">
      <!-- ★ Epic 1 P1: Skeleton loader -->
      <div v-if="replay.loading" class="clip-grid" :style="{ gridTemplateColumns: `repeat(${gridCols}, 1fr)` }">
        <div v-for="i in gridCols * 2" :key="'sk'+i" class="skeleton-card">
          <div class="skeleton-thumb animate-pulse"></div>
          <div class="skeleton-info">
            <div class="skeleton-line w70 animate-pulse"></div>
            <div class="skeleton-line w40 animate-pulse"></div>
          </div>
        </div>
      </div>

      <!-- ★ Epic 1 P1b: Staggered fade-in -->
      <div v-else-if="replay.filteredClips.length > 0" class="clip-grid" :style="{ gridTemplateColumns: `repeat(${gridCols}, 1fr)` }">
        <ClipCard v-for="(clip, idx) in replay.filteredClips" :key="clip.id" :clip="clip"
          :selected="replay.isSelected(clip.id)"
          class="clip-stagger" :style="{ animationDelay: `${idx * 50}ms` }"
          @click="onCardClick(clip)" @preview="openPreview" @editor="openAdvanced"
          @rename="startRename" @delete="deleteClip" />
      </div>

      <div v-else class="empty-state">
        <div class="empty-ic">{{ replay.filterFav?'❤':replay.search?'🔍':'📁' }}</div>
        <p v-if="replay.search">No clips matching "{{ replay.search }}"</p>
        <p v-else-if="replay.filterFav">No favorited clips</p>
        <template v-else><p>No clips found</p><p class="empty-sub">{{ persist.state?.settings?.clipsFolder || '~/Videos/OpenGG' }}</p></template>
      </div>
    </div>

    <!-- Multi-select -->
    <Transition name="slide-up">
      <div v-if="replay.selectMode" class="sel-bar"><span>{{ replay.selectedCount }} selected</span><div style="flex:1"></div><button class="sel-btn" @click="replay.clearSelection()">Clear</button><button class="sel-btn sel-btn-d" @click="deleteSelected()">🗑 Delete</button></div>
    </Transition>

    <Teleport to="body">
      <div v-if="renameTarget" class="dlg-ov" @click.self="renameTarget=null"><div class="dlg"><h3>Rename Clip</h3><input v-model="renameValue" class="dlg-in" @keydown.enter="confirmRename" autofocus /><div class="dlg-btns"><button class="dlg-btn" @click="renameTarget=null">Cancel</button><button class="dlg-btn dlg-pri" @click="confirmRename">Save</button></div></div></div>
    </Teleport>

    <Transition name="fade"><div v-if="toast" class="toast">{{ toast }}</div></Transition>
    <ClipEditor v-if="editorClip&&!advancedClip" :clip="editorClip" :mode="editorMode" @close="editorClip=null" @saved="refreshClips" @toast="showToast" />
    <AdvancedEditor v-if="advancedClip" :clip="advancedClip" @close="advancedClip=null" />
  </div>
</template>

<style scoped>
.page { display:flex; flex-direction:column; gap:14px; height:100%; }
.header { display:flex; align-items:center; justify-content:space-between; flex-shrink:0; }
.title { font-size:22px; font-weight:700; }
.header-r { display:flex; align-items:center; gap:8px; }
.rec-dot { width:8px; height:8px; border-radius:50%; background:var(--text-muted); } .rec-dot.active { background:var(--danger); animation:pulse 1.2s infinite; }
@keyframes pulse { 0%,100%{opacity:1} 50%{opacity:.5} }
.rec-txt { font-size:12px; color:var(--text-sec); }
.ib { width:30px; height:30px; border:1px solid var(--border); border-radius:6px; background:var(--bg-card); color:var(--text-sec); cursor:pointer; display:flex; align-items:center; justify-content:center; font-size:13px; } .ib:hover { background:var(--bg-hover); } .ib:disabled { opacity:.4; }

.ctrl-bar { display:flex; align-items:center; gap:10px; flex-wrap:wrap; flex-shrink:0; }
.search-wrap { flex:1; min-width:180px; max-width:360px; position:relative; }
.search-ic { position:absolute; left:10px; top:50%; transform:translateY(-50%); width:15px; height:15px; color:var(--text-muted); pointer-events:none; }
.search { width:100%; padding:7px 12px 7px 32px; background:var(--bg-card); border:1px solid var(--border); border-radius:7px; color:var(--text); font-size:13px; outline:none; color-scheme:dark; } .search:focus { border-color:var(--accent); } .search::placeholder { color:var(--text-muted); }
.ctrl-grp { display:flex; align-items:center; gap:5px; }
.ctrl-grp label { font-size:10px; font-weight:700; color:var(--text-muted); text-transform:uppercase; letter-spacing:.5px; }
.sel { padding:6px 8px; background:var(--bg-card); border:1px solid var(--border); border-radius:6px; color:var(--text-sec); font-size:12px; outline:none; cursor:pointer; color-scheme:dark; }
.sel-sm { width:52px; }
.fav-btn { padding:6px 10px; border:1px solid var(--border); border-radius:6px; background:transparent; color:var(--text-sec); font-size:12px; font-weight:600; cursor:pointer; } .fav-btn:hover { background:var(--bg-hover); } .fav-btn.active { background:var(--accent); border-color:var(--accent); color:#fff; }

/*
  ★ FIX 3: SCROLL CONTAINER
  - Takes all remaining vertical space via flex:1
  - overflow-y:auto enables scrolling
  - The grid inside flows naturally downward
*/
.scroll-area {
  flex: 1;
  min-height: 0;      /* ← Required for flex children to shrink and enable scroll */
  overflow-y: auto;
  padding-bottom: 16px;
}

/*
  ★ FIX 3: GRID
  - Columns set via :style binding (3/4/5)
  - grid-auto-rows: max-content → cards are their natural height (not squished)
  - NO height constraints on the grid itself
*/
.clip-grid {
  display: grid;
  gap: 16px;
  grid-auto-rows: max-content;
  align-content: start;
}

.empty-state { display:flex; flex-direction:column; align-items:center; justify-content:center; color:var(--text-muted); padding:40px; min-height:200px; }
.empty-ic { font-size:36px; margin-bottom:10px; opacity:.4; }
.empty-sub { font-size:12px; opacity:.6; }

/* ★ Epic 1 P1: Skeleton loader */
.skeleton-card { border-radius:8px; overflow:hidden; background:var(--bg-card); border:1px solid var(--border); }
.skeleton-thumb { aspect-ratio:16/9; background:var(--bg-deep); }
.skeleton-info { padding:10px; display:flex; flex-direction:column; gap:6px; }
.skeleton-line { height:10px; background:var(--bg-deep); border-radius:4px; }
.w70 { width:70%; }
.w40 { width:40%; }
.animate-pulse { animation:pulse 1.5s ease-in-out infinite; }
@keyframes pulse { 0%,100%{opacity:1} 50%{opacity:.4} }

/* ★ Epic 1 P1b: Staggered fade-in */
.clip-stagger { animation:fadeSlideIn .3s ease both; }
@keyframes fadeSlideIn { from{opacity:0;transform:translateY(8px)} to{opacity:1;transform:translateY(0)} }

.sel-bar { position:sticky; bottom:0; display:flex; align-items:center; gap:12px; padding:12px 20px; background:var(--bg-card); border-top:1px solid var(--border); border-radius:12px 12px 0 0; box-shadow:0 -4px 16px rgba(0,0,0,.3); z-index:50; font-size:13px; font-weight:600; color:var(--text-sec); }
.sel-btn { padding:7px 14px; border:1px solid var(--border); border-radius:6px; background:var(--bg-card); color:var(--text-sec); font-size:12px; cursor:pointer; } .sel-btn:hover { background:var(--bg-hover); }
.sel-btn-d { color:var(--danger); border-color:var(--danger); } .sel-btn-d:hover { background:rgba(220,38,38,.1); }
.slide-up-enter-active,.slide-up-leave-active { transition:transform .2s,opacity .2s; } .slide-up-enter-from,.slide-up-leave-to { transform:translateY(100%); opacity:0; }

.dlg-ov { position:fixed; inset:0; z-index:2000; background:rgba(0,0,0,.6); display:flex; align-items:center; justify-content:center; }
.dlg { background:var(--bg-surface); border:1px solid var(--border); border-radius:10px; padding:24px; width:400px; }
.dlg h3 { font-size:16px; font-weight:700; margin-bottom:16px; }
.dlg-in { width:100%; padding:10px 14px; background:var(--bg-input); border:1px solid var(--border); border-radius:6px; color:var(--text); font-size:14px; outline:none; color-scheme:dark; } .dlg-in:focus { border-color:var(--accent); }
.dlg-btns { display:flex; gap:8px; justify-content:flex-end; margin-top:16px; }
.dlg-btn { padding:8px 18px; border:1px solid var(--border); border-radius:6px; background:var(--bg-card); color:var(--text-sec); font-size:13px; cursor:pointer; } .dlg-btn:hover { background:var(--bg-hover); }
.dlg-pri { background:var(--accent); border-color:var(--accent); color:#fff; }
.toast { position:fixed; bottom:20px; left:50%; transform:translateX(-50%); background:var(--bg-card); border:1px solid var(--accent); color:var(--text); padding:10px 24px; border-radius:8px; font-size:13px; font-weight:600; z-index:9999; box-shadow:0 4px 16px rgba(0,0,0,.3); }
.fade-enter-active,.fade-leave-active { transition:opacity .3s; } .fade-enter-from,.fade-leave-to { opacity:0; }
</style>
