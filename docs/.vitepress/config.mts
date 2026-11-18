import { defineConfig } from "vitepress";

const repository = "https://github.com/dengfuping/cursor-byok";

const zhText = {
  start: "开始使用",
  config: "配置",
  help: "疑难",
  overview: "概览与快速开始",
  install: "下载与安装",
  models: "模型通道",
  tab: "TAB 补全",
  settings: "系统设置",
  troubleshooting: "报错排查",
  faq: "常见问题"
};

const enText = {
  start: "Getting started",
  config: "Configuration",
  help: "Help",
  overview: "Overview and quick start",
  install: "Install",
  models: "Model channels",
  tab: "TAB completion",
  settings: "Settings",
  troubleshooting: "Troubleshooting",
  faq: "FAQ"
};

function sidebar(text: typeof zhText, prefix: string) {
  return [
    {
      text: text.start,
      items: [
        { text: text.overview, link: `${prefix}/` },
        { text: text.install, link: `${prefix}/install` }
      ]
    },
    {
      text: text.config,
      items: [
        { text: text.models, link: `${prefix}/models` },
        { text: text.tab, link: `${prefix}/tab` },
        { text: text.settings, link: `${prefix}/settings` }
      ]
    },
    {
      text: text.help,
      items: [
        { text: text.troubleshooting, link: `${prefix}/troubleshooting` },
        { text: text.faq, link: `${prefix}/faq` }
      ]
    }
  ];
}

export default defineConfig({
  title: "cursor-byok",
  description: "在 Cursor 中使用你自己配置的模型服务",
  base: "/cursor-byok/",
  cleanUrls: true,
  lastUpdated: true,
  themeConfig: {
    logo: undefined,
    socialLinks: [{ icon: "github", link: repository }],
    editLink: { pattern: `${repository}/edit/main/docs/:path` },
    // Shared provider lives at the root so the search bundle is always loaded;
    // locales only override translations.
    search: { provider: "local" }
  },
  locales: {
    root: {
      label: "简体中文",
      lang: "zh-CN",
      themeConfig: {
        nav: [
          { text: "使用指南", link: "/install" },
          { text: "排错", link: "/troubleshooting" },
          { text: "下载", link: `${repository}/releases/latest` },
          { text: "Issues", link: `${repository}/issues` }
        ],
        sidebar: sidebar(zhText, ""),
        search: {
          options: {
            translations: {
              button: { buttonText: "搜索", buttonAriaLabel: "搜索" },
              modal: {
                noResultsText: "没有找到相关内容",
                resetButtonTitle: "清除查询",
                footer: { selectText: "选择", navigateText: "切换", closeText: "关闭" }
              }
            }
          }
        },
        outline: { level: [2, 3], label: "本页目录" },
        editLink: { text: "在 GitHub 上修改此页" },
        docFooter: { prev: "上一页", next: "下一页" },
        footer: {
          message: "以 MIT 许可证开源。本项目与 Cursor 官方无关。",
          copyright: "cursor-byok"
        }
      }
    },
    en: {
      label: "English",
      lang: "en-US",
      link: "/en/",
      themeConfig: {
        nav: [
          { text: "Guide", link: "/en/install" },
          { text: "Troubleshooting", link: "/en/troubleshooting" },
          { text: "Download", link: `${repository}/releases/latest` },
          { text: "Issues", link: `${repository}/issues` }
        ],
        sidebar: sidebar(enText, "/en"),
        outline: { level: [2, 3], label: "On this page" },
        editLink: { text: "Edit this page on GitHub" },
        docFooter: { prev: "Previous", next: "Next" },
        footer: {
          message: "Released under the MIT License. Not affiliated with Cursor.",
          copyright: "cursor-byok"
        }
      }
    }
  }
});
