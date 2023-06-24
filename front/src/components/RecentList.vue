<script setup>
import { NDataTable, NCard, NIcon, NButton, NText } from "naive-ui";
import { useRouter } from "vue-router";
import { h, reactive, ref } from "vue";

import { ArrowSync20Filled } from "@vicons/fluent";

const router = useRouter();

const column_modified_at = reactive({
  title: "修改时间",
  key: "modified_at",
  render(row) {
    // Parse Unix timestamp to human readable string
    return new Date(row.modified_at).toLocaleString();
  },
  sorter: true,
  sortOrder: "descend",
});

const column_num = reactive({
  title: "数量",
  key: "num",
  sorter: true,
});

const columns = ref([
  column_modified_at,
  column_num,
  {
    title: "备注",
    key: "remark",
    default: "无",
  },
  {
    title: "操作",
    key: "actions",
    render(row) {
      return [
        h(
          NButton,
          {
            type: "default",
            size: "small",
            style: "margin-right: 5px",
            onClick: () => router.push(`/editor/${row.id}`),
          },
          () => "编辑"
        ),
        h(
          NButton,
          {
            type: "error",
            size: "small",
            onClick: () => deleteDetection(row.id),
          },
          () => "删除"
        ),
      ];
    },
  },
]);

function renderCell(value) {
  if (!value) {
    return h(NText, { depth: 3 }, { default: () => "无" });
  }
  return value;
}

const data = ref([]);

const loading = ref(true);

const pagination = reactive({
  page: 1,
  pageSize: 10,
  pageCount: 1,
  prefix: ({ itemCount }) => `共 ${itemCount} 条`,
});

async function fetchData() {
  loading.value = true;
  let sort_key = "modified_at";
  let asc = false;

  if (column_num.sortOrder) {
    sort_key = "num";
    asc = column_num.sortOrder === "ascend";
  }

  if (column_modified_at.sortOrder) {
    sort_key = "modified_at";
    asc = column_modified_at.sortOrder === "ascend";
  }

  const res = await fetch(
    "/api/detections?" +
      new URLSearchParams({
        page: pagination.page,
        page_size: pagination.pageSize,
        sort_key,
        asc,
      })
  );

  const body = await res.json();
  pagination.pageCount = ~~res.headers.get("pages");
  pagination.itemCount = ~~res.headers.get("count");
  console.log(body);
  if (body.length) data.value = body;
  else data.value = [];
  loading.value = false;
}

fetchData();

function deleteDetection(id) {
  fetch(`/api/detections/${id}`, {
    method: "DELETE",
  }).then(fetchData);
}
</script>

<template>
  <n-card title="最近的检测">
    <template #header-extra>
      <n-button circle @click="fetchData">
        <template #icon>
          <n-icon>
            <ArrowSync20Filled />
          </n-icon>
        </template>
      </n-button>
    </template>

    <n-data-table
      remote
      :columns="columns"
      :row-key="(row) => row.id"
      :data="data"
      :render-cell="renderCell"
      :loading="loading"
      :pagination="pagination"
      @update:page="
        (page) => {
          pagination.page = page;
          fetchData();
        }
      "
      @update:sorter="
        (sorter) => {
          console.log(sorter);
          if (sorter.columnKey === 'modified_at') {
            column_num.sortOrder = false;
            column_modified_at.sorter = true;
            column_modified_at.sortOrder = sorter.order;
          } else {
            column_modified_at.sortOrder = false;
            column_num.sorter = true;
            column_num.sortOrder = sorter.order;
          }
          fetchData();
        }
      "
    ></n-data-table>
  </n-card>
</template>
