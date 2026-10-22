# 图标使用说明

最后核对：2026-10-22

## 顶部导航

来源：`ui/shell/top-bar.slint`

| 文本 | 图标 |
| --- | --- |
| 启动 | `lightning-charge.svg` |
| 实例 | `instance.svg` |
| 账户 | `account.svg` |
| 发现 | `compass.svg` |
| 设置 | `setting.svg` |

品牌文字 `Neutrino` 左侧使用代码绘制的渐变方块 `N`，不使用 `icons/` 中的图标。

## 启动页

来源：`ui/views/launch-view.slint`

| 文本 | 图标 |
| --- | --- |
| 角色 | `account.svg` |
| 实例 | `instance.svg` |

## 实例页

来源：`ui/views/instances-view.slint`

### 左侧菜单

| 文本 | 图标 |
| --- | --- |
| 全部实例 | `boxes.svg` |
| 实例文件夹名（如 `26.1-mc`） | `instance.svg` |

### 实例卡片

| 文本 | 图标 |
| --- | --- |
| 实例文件夹名 | `instance.svg` |

### 实例详情标签

| 文本 | 图标 |
| --- | --- |
| 首页 | `home.svg` |
| 世界 | `map.svg` |
| 模组 | `dice-5.svg` |
| 资源包 | `resource.svg` |
| 原理图 | `schematic.svg` |
| 光影 | `shader.svg` |

## 账户页

来源：`ui/views/account-view.slint`

| 文本 | 图标 |
| --- | --- |
| 全部来源 | `people.svg` |
| 微软账号 | `microsoft.svg` |
| 离线账号 | `offline.svg` |

## 设置页

来源：`ui/views/settings-view.slint`

### 左侧菜单

| 文本 | 图标 |
| --- | --- |
| 全局游戏设置 | `game.svg` |
| Java | `cup-hot.svg` |
| 通用 | `setting.svg` |
| 外观 | `palette.svg` |
| 下载页 | `cloud-download.svg` |
| 文档与帮助 | `question-circle.svg` |
| 关于 | `exclamation-circle.svg` |

### 设置内容中的图标

| 文本或用途 | 图标 |
| --- | --- |
| 官方启动文件夹 | `folder.svg` |
| 打开游戏目录按钮 | `folder2-open.svg` |
| 打开 Java 目录按钮 | `folder2-open.svg` |
| 文档与帮助外链按钮 | `box-arrow-up-right.svg` |

## 备注

- `folder.svg` 用于标识目录，`folder2-open.svg` 用于“打开文件夹”操作。
- `jdk/` 子目录保留各发行商 Logo，但当前 Java 信息卡片已不显示发行商 Logo。
