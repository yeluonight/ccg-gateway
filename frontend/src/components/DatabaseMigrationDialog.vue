<template>
  <el-dialog
    v-model="visible"
    title="数据库优化"
    width="500px"
    :close-on-click-modal="false"
    :close-on-press-escape="false"
    :show-close="false"
  >
    <div class="dialog-content">
      <el-icon class="warning-icon" :size="48"><WarningFilled /></el-icon>
      <p class="message">检测到数据库未启用自动空间回收功能</p>
      <p class="description">
        启用后，删除日志时会自动释放磁盘空间。<br>
        需要重建数据库（约几秒钟），期间会短暂锁定数据库。
      </p>
      <el-checkbox v-model="remember">不再提示（我将手动处理）</el-checkbox>
    </div>
    <template #footer>
      <el-button type="primary" :loading="migrating" @click="handleMigrate">
        立即优化
      </el-button>
      <el-button @click="handleSkip" :disabled="migrating">
        跳过
      </el-button>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { ElMessage } from 'element-plus'
import { WarningFilled } from '@element-plus/icons-vue'
import { settingsApi } from '@/api/settings'

const visible = defineModel<boolean>({ required: true })
const remember = ref(false)
const migrating = ref(false)

const emit = defineEmits<{
  (e: 'done'): void
}>()

async function handleMigrate() {
  migrating.value = true
  try {
    await settingsApi.migrateDatabase()
    ElMessage.success('数据库优化完成')
    if (remember.value) {
      localStorage.setItem('db-migration-dismissed', '1')
    }
    visible.value = false
    emit('done')
  } catch (err) {
    ElMessage.error('优化失败，请重试')
  } finally {
    migrating.value = false
  }
}

function handleSkip() {
  if (remember.value) {
    localStorage.setItem('db-migration-dismissed', '1')
  }
  visible.value = false
}
</script>

<style scoped>
.dialog-content {
  text-align: center;
  padding: 20px 0;
}

.warning-icon {
  color: #e6a23c;
  margin-bottom: 16px;
}

.message {
  font-size: 16px;
  font-weight: 500;
  margin: 0 0 12px;
}

.description {
  color: #606266;
  font-size: 14px;
  margin: 0 0 20px;
  line-height: 1.6;
}
</style>
