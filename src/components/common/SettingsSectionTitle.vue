<template>
  <h3 class="settings-section-title"><slot /></h3>
</template>

<style scoped lang="scss">
.settings-section-title {
  margin: 0 0 16px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border-color);
  color: var(--text-primary);
  font-size: var(--font-size-base);
  line-height: 1.5;
  font-weight: 600;
}
</style>
