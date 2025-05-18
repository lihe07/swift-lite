<script setup>
import { NTimeline, NTimelineItem, NScrollbar } from 'naive-ui'
</script>

<template>
  <n-timeline class="timeline">
    <n-timeline-item type="" title="1.0.2" time="2025-05-18">
      修复防火墙, 连接数过多导致的崩溃问题.
    </n-timeline-item>
    <n-timeline-item type="" title="1.0.1" time="2024-03-23">
      加入分页和排序, 列表动态更新. 增加检测集群数量. 修改默认参数.
    </n-timeline-item>
    <n-timeline-item type="success" title="1.0.0" time="2024-03-22">
      修复多人同时编辑, 忙时编辑造成的冲突. 修复图片刷新缩放重置.
      <br />
      优化网络请求. 加入更新日志.
    </n-timeline-item>
    <n-timeline-item type="" title="1.0.0-alpha" content="测试版本上线" time="2024-03-14" />
  </n-timeline>
</template>

<style>
.timeline .n-timeline-item-content__content {
  line-height: 1.5rem;
}
</style>
