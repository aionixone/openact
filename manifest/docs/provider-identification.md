# OpenAct Provider Identification

## 概述

OpenAct 系统完全基于 OpenAPI 文档的元数据来识别提供商，不硬编码任何特定的提供商信息。这确保了系统的灵活性和可扩展性。

## 提供商识别优先级

### 1. x-provider 扩展字段（推荐）

在 OpenAPI 规范中明确指定提供商：

```yaml
openapi: 3.0.0
info:
  title: "My API"
  version: "1.0.0"
x-provider: "slack"  # 明确指定提供商
```

### 2. x-vendor 扩展字段（备选）

```yaml
openapi: 3.0.0
info:
  title: "My API"
  version: "1.0.0"
x-vendor: "github"  # 备选方式
```

### 3. x-service 扩展字段（备选）

```yaml
openapi: 3.0.0
info:
  title: "My API"
  version: "1.0.0"
x-service: "custom-service"  # 备选方式
```

### 4. 从 servers 域名自动识别

```yaml
openapi: 3.0.0
info:
  title: "My API"
  version: "1.0.0"
servers:
  - url: "https://api.example.com/v1"
    description: "Production server"
```

系统会自动从 `api.example.com` 提取 `example` 作为提供商名称。

### 5. 从 title 自动提取（最后手段）

```yaml
openapi: 3.0.0
info:
  title: "Slack API"  # 自动提取 "slack"
  version: "1.0.0"
```

## 域名处理规则

系统会自动处理常见的域名模式：

- `api.example.com` → `example`
- `www.example.com` → `example`
- `example-api.com` → `example`
- `example.com` → `example`

## 标题处理规则

系统会自动清理标题中的常见词汇：

- "Slack API" → `slack`
- "GitHub REST API" → `github`
- "Custom Web Service" → `custom`

## 示例

### 明确指定提供商

```yaml
openapi: 3.0.0
info:
  title: "Slack API"
  version: "1.0.0"
x-provider: "slack"
```

生成的 TRN：
```
trn:openact:tenant123:action/chat.postMessage:provider/slack
```

### 从域名识别

```yaml
openapi: 3.0.0
info:
  title: "Custom API"
  version: "1.0.0"
servers:
  - url: "https://api.mycompany.com/v1"
```

生成的 TRN：
```
trn:openact:tenant123:action/users.info:provider/mycompany
```

### 从标题识别

```yaml
openapi: 3.0.0
info:
  title: "GitHub API"
  version: "1.0.0"
```

生成的 TRN：
```
trn:openact:tenant123:action/repos.issues:provider/github
```

## 最佳实践

1. **推荐使用 x-provider**：在 OpenAPI 规范中明确指定提供商名称
2. **保持一致性**：在整个项目中使用相同的提供商命名
3. **使用小写**：提供商名称会自动转换为小写
4. **避免特殊字符**：只使用字母、数字和连字符

## 扩展性

这种设计确保了：

- ✅ **无硬编码**：不依赖任何特定的提供商列表
- ✅ **完全可配置**：通过 OpenAPI 扩展字段控制
- ✅ **向后兼容**：支持多种识别方式
- ✅ **易于扩展**：添加新的识别方式无需修改代码
