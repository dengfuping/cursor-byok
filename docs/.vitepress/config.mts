import { defineConfig } from "vitepress";

const repository = "https://github.com/dengfuping/cursor-byok";

export default defineConfig({
  lang: "zh-CN",
  title: "cursor-byok",
  description: "在 Cursor 中使用你自己配置的模型服务",
  base: "/cursor-byok/",
  cleanUrls: true,
  lastUpdated: true,
  themeConfig: {
    nav: [
      { text: "使用指南", link: "/install" },
      { text: "排错", link: "/troubleshooting" },
      { text: "下载", link: `${repository}/releases/latest` },
      { text: "Issues", link: `${repository}/issues` }
    ],
    sidebar: [
      {
        text: "开始使用",
        items: [
          { text: "概览与快速开始", link: "/" },
          { text: "下载与安装", link: "/install" }
        ]
      },
      {
        text: "配置",
        items: [
          { text: "模型通道", link: "/models" },
          { text: "TAB 补全", link: "/tab" },
          { text: "系统设置", link: "/settings" }
        ]
      },
      {
        text: "疑难",
        items: [
          { text: "报错排查", link: "/troubleshooting" },
          { text: "常见问题", link: "/faq" }
        ]
      }
    ],
    outline: { level: [2, 3], label: "本页目录" },
    search: { provider: "local" },
    editLink: {
      pattern: `${repository}/edit/main/docs/:path`,
      text: "在 GitHub 上修改此页"
    },
    docFooter: { prev: "上一页", next: "下一页" },
    socialLinks: [{ icon: "github", link: repository }],
    footer: {
      message: "以 MIT 许可证开源。本项目与 Cursor 官方无关。",
      copyright: "cursor-byok"
    }
  }
});
