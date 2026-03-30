<script setup lang="ts">
// ★ Epic 5: Devices page — coming in a future release
</script>

<template>
  <div class="coming-soon">
    <div class="cs-inner">
      <div class="cs-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2">
          <path d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z"/>
        </svg>
      </div>
      <h2 class="cs-title">Coming Soon</h2>
      <p class="cs-subtitle">قادم قريباً</p>
      <p class="cs-desc">
        Devices &amp; RGB control — gaming peripherals, OpenRGB integration,
        and auto-profile switching are on the roadmap.
      </p>
      <span class="cs-badge">Beta</span>
    </div>
  </div>
</template>

<style scoped>
.coming-soon {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-main);
}
.cs-inner {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  max-width: 400px;
  text-align: center;
  padding: 48px 32px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 16px;
}
.cs-icon {
  width: 64px; height: 64px;
  border-radius: 16px;
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
  display: flex; align-items: center; justify-content: center;
  color: var(--accent);
  margin-bottom: 8px;
}
.cs-icon svg { width: 30px; height: 30px; }
.cs-title {
  font-size: 22px; font-weight: 800; color: var(--text); margin: 0;
}
.cs-subtitle {
  font-size: 16px; font-weight: 500; color: var(--text-sec);
  margin: 0; direction: rtl;
}
.cs-desc {
  font-size: 13px; color: var(--text-muted); line-height: 1.6; margin: 8px 0 4px;
}
.cs-badge {
  display: inline-block;
  background: color-mix(in srgb, var(--accent) 15%, transparent);
  color: var(--accent);
  border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  font-size: 10px; font-weight: 700; letter-spacing: 1px;
  padding: 3px 10px; border-radius: 20px; text-transform: uppercase;
}
</style>
