# AuthFlow DSL Templates

目录结构:
- templates/providers/<provider>/<template>.json

命名规范:
- provider: 小写英文 (github, google, microsoft)
- template: 认证类型或用途 (oauth2, client_credentials, refresh_token, saml)

占位符:
- __CLIENT_ID__, __CLIENT_SECRET__, __REDIRECT_URI__ 等

使用方式:
- 直接上传该模板到 /api/v1/workflows 时, 先替换占位符;
- 或由前端加载模板并渲染表单, 填写后提交创建工作流。
