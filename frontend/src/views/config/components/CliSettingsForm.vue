<template>
  <el-form :model="form" label-width="0">
    <el-form-item>
      <el-input
        v-model="form.default_json_config"
        type="textarea"
        :rows="10"
        :placeholder="placeholder"
      />
      <div class="form-tip">{{ tip }}</div>
    </el-form-item>
    <el-form-item>
      <el-button type="primary" @click="handleSave">保存</el-button>
    </el-form-item>
  </el-form>
</template>

<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import type { CliSettings } from '@/types/models'

const props = defineProps<{
  cliType: string
  settings?: CliSettings
}>()

const emit = defineEmits<{
  save: [cliType: string, data: { default_json_config: string }]
}>()

const form = ref({
  default_json_config: ''
})

const placeholder = computed(() => {
  switch (props.cliType) {
    case 'codex':
      return `model_reasoning_effort = "high"
model_reasoning_summary = "detailed"`
    case 'claude_code':
      return `{
  "env": {},
  "permissions": {}
}`
    case 'gemini':
      return `{
  "theme": "dark"
}`
    default:
      return '{}'
  }
})

const tip = computed(() => {
  switch (props.cliType) {
    case 'codex':
      return '此处配置会合并到 ~/.codex/config.toml（TOML 格式）'
    case 'claude_code':
      return '此处配置会合并到 ~/.claude/settings.json（JSON 格式）'
    case 'gemini':
      return '此处配置会合并到 ~/.gemini/settings.json（JSON 格式）'
    default:
      return '此处配置会合并到 CLI 的配置文件中'
  }
})

watch(() => props.settings, (settings) => {
  if (settings) {
    form.value = {
      default_json_config: settings.default_json_config
    }
  }
}, { immediate: true })

function handleSave() {
  emit('save', props.cliType, form.value)
}
</script>

<style scoped>
.form-tip {
  margin-top: 5px;
  color: #999;
  font-size: 12px;
}
</style>
