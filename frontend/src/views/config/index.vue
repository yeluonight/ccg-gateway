<template>
  <div class="config-page">
    <el-row :gutter="20">
      <!-- Timeout Settings -->
      <el-col :span="12">
        <el-card class="config-card">
          <template #header>超时配置</template>
          <el-form :model="timeoutForm" label-width="140px">
            <el-form-item label="流式首字节超时">
              <el-input-number v-model="timeoutForm.stream_first_byte_timeout" :min="1" />
              <span class="unit">秒</span>
            </el-form-item>
            <el-form-item label="流式空闲超时">
              <el-input-number v-model="timeoutForm.stream_idle_timeout" :min="1" />
              <span class="unit">秒</span>
            </el-form-item>
            <el-form-item label="非流式超时">
              <el-input-number v-model="timeoutForm.non_stream_timeout" :min="1" />
              <span class="unit">秒</span>
            </el-form-item>
            <el-form-item>
              <el-button type="primary" @click="saveTimeouts">保存</el-button>
            </el-form-item>
          </el-form>
        </el-card>
      </el-col>

      <!-- CLI Settings -->
      <el-col :span="12">
        <el-card class="config-card">
          <template #header>CLI 配置</template>
          <el-tabs v-model="activeCliTab">
            <el-tab-pane label="ClaudeCode" name="claude_code">
              <CliSettingsForm cli-type="claude_code" :settings="settingsStore.settings?.cli_settings?.claude_code" @save="saveCli" />
            </el-tab-pane>
            <el-tab-pane label="Codex" name="codex">
              <CliSettingsForm cli-type="codex" :settings="settingsStore.settings?.cli_settings?.codex" @save="saveCli" />
            </el-tab-pane>
            <el-tab-pane label="Gemini" name="gemini">
              <CliSettingsForm cli-type="gemini" :settings="settingsStore.settings?.cli_settings?.gemini" @save="saveCli" />
            </el-tab-pane>
          </el-tabs>
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { ElMessage } from 'element-plus'
import { useSettingsStore } from '@/stores/settings'
import CliSettingsForm from './components/CliSettingsForm.vue'

const settingsStore = useSettingsStore()
const activeCliTab = ref('claude_code')

const timeoutForm = ref({
  stream_first_byte_timeout: 30,
  stream_idle_timeout: 60,
  non_stream_timeout: 120
})

watch(() => settingsStore.settings, (settings) => {
  if (settings) {
    timeoutForm.value = { ...settings.timeouts }
  }
}, { immediate: true })

async function saveTimeouts() {
  await settingsStore.updateTimeouts(timeoutForm.value)
  ElMessage.success('超时配置已保存')
}

async function saveCli(cliType: string, data: any) {
  await settingsStore.updateCli(cliType, data)
  ElMessage.success('CLI 配置已保存')
}

onMounted(() => {
  settingsStore.fetchSettings()
})
</script>

<style scoped>
.unit {
  margin-left: 10px;
  color: #999;
}
.config-card {
  height: 100%;
}
</style>
