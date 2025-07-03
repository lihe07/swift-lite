<script setup>
import { NTimeline, NTimelineItem, NScrollbar } from 'naive-ui'
</script>

<template>
  <n-timeline class="timeline">
    <n-timeline-item type="success" title="1.0.6" time="2025-07-03">
      计算节点信息展示. 服务器扩容.
    </n-timeline-item>
    <n-timeline-item type="" title="1.0.5" time="2025-06-29">
      优化轮询, 优化服务器网络, 增加"创建时间"列, 保存顺序信息.
    </n-timeline-item>
    <n-timeline-item type="" title="1.0.4" time="2025-06-24">
      性能优化, 修复卡顿, 升级各种依赖包.
    </n-timeline-item>
    <n-timeline-item type="" title="1.0.3" time="2025-05-22">
      修复桌面端编辑器界面元素溢出.
    </n-timeline-item>
    <n-timeline-item type="" title="1.0.2" time="2025-05-18">
      修复防火墙, 连接数过多导致的崩溃问题.
    </n-timeline-item>
    <n-timeline-item type="" title="1.0.1" time="2024-03-23">
      加入分页和排序, 列表动态更新. 增加检测集群数量. 修改默认参数.
    </n-timeline-item>
    <n-timeline-item type="" title="1.0.0" time="2024-03-22">
      修复多人同时编辑, 忙时编辑造成的冲突. 修复图片刷新缩放重置.
      <br />
      优化网络请求. 加入更新日志.
    </n-timeline-item>
    <n-timeline-item type="" title="1.0.0-alpha" content="LITE 测试版本上线" time="2024-03-14" />
    <n-timeline-item type="" title="0.1.0" content="早期版本上线" time="2021-06-23" />
  </n-timeline>
</template>

<style>
.timeline .n-timeline-item-content__content {
  line-height: 1.5rem;
}
</style>
