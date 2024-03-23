<script setup>
import { NDataTable, NButtonGroup, NButton, NText } from "naive-ui";
import { useRouter } from "vue-router";
import { h, ref, reactive, onUnmounted } from "vue";
import { io } from "socket.io-client";




const router = useRouter();

const columns = ref([
  {
    title: "修改时间",
    key: "modified_at",
    render(row) {
      // Parse Unix timestamp to human readable string
      return new Date(row.modified_at * 1000).toLocaleString();
    },
    sorter: true,
    defaultSortOrder: "descend",
    sortOrder: "descend",
  },
  {
    title: "状态",
    key: "status",
    render(row) {
      return { done: "完成", queue: "排队中", processing: "处理中" }[row.status];
    },
    sorter: true,
    sortOrder: false,
  },
  {
    title: "数量",
    key: "num",
    sorter: true,
    sortOrder: false,
  },
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
const pagination = reactive({
  pageSizes: [5, 10, 20, 50],
  pageSize: 20,
  page: 1,
  showSizePicker: true,
  onChange(page) {
    pagination.page = page;
    fetchData();
  },
  onUpdatePageSize(size) {
    pagination.pageSize = size;
    fetchData();
  },
});

const loading = ref(true);
const table = ref(null);

async function fetchData(silent = false) {
  // console.log(columns.value?.map(col => col.sortOrder))
  let sortby = 'modified_at'
  let sort = 'desc'

  for (const col of columns.value) {
    if (col.sortOrder) {
      sortby = col.key
      sort = col.sortOrder
      break
    }
  }

  sort = sort === 'ascend' ? 'asc' : 'desc'

  loading.value = !silent;
  const resp = await fetch("/api/detections?" + new URLSearchParams({ page: pagination.page, size: pagination.pageSize, sortby, sort }))
  const json = await resp.json()
  console.log(resp);
  pagination.pageCount = Math.ceil(json.total / pagination.pageSize);
  pagination.itemCount = json.total;

  console.log(pagination.pageCount, pagination.itemCount)
  data.value = json.data;
  loading.value = false;
}

fetchData();

const socket = io({
  path: "/api/ws",
  addTrailingSlash: false
})


socket.on("update_detection", () => fetchData(true))
socket.on("new_detection", () => fetchData(true))

socket.on("connect", () => {
  console.log("Connected");
  socket.emit("join", "all");
});


onUnmounted(() => {
  socket.disconnect();
});


function handleSorterChange(sorter) {
  console.log(sorter)
  if (!sorter.order) {
    sorter.order = "descend";
  }

  columns.value = columns.value.map((col) => {
    if (col.key === sorter.columnKey) {
      return {
        ...col,
        sortOrder: sorter.order,
      };
    }
    return {
      ...col,
      sortOrder: false,
    };
  })

  fetchData();

}

function deleteDetection(id) {
  fetch(`/api/detections/${id}`, {
    method: "DELETE",
  }).then(fetchData);
}
</script>

<template>
  <n-data-table remote ref="table" :columns="columns" :data="data" :render-cell="renderCell" :loading="loading"
    @update:sorter="handleSorterChange" :pagination="pagination"></n-data-table>
</template>
