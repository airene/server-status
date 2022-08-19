# ServerStatus 云探针

## 动机

- 起初为了学习 rust 的玩具项目，不知道有没有以后
- v1.x 是能用版
- v2 开始做项目级别的refactor
- `copy`自[ServerStatus-Rust](https://github.com/zdz/ServerStatus-Rust)项目,因为要改动很多东西，所以没有走fork

## 和 ServerStatus-Rust 的差异

- 前端web： 删掉了 bootstrap,jquery, jinja，精简返回json字段（依赖删除功能），**只依赖一个html文件**
- 功能变动： 
  - 删除通知功能
  - 删除分组功能
  - 删除 detail 和 map 页面和功能（并删除后台管理的概念）
  - 精简上报数据结构（~~load avg,线程，ping值，交换内存等~~）
  - 删除几点下线功能（改为主机最后更新时间）
  - 删除月流量概念
  - 删除ip上报，刷新ip，cloud部署功能
  
- 项目依赖： 升级最新的prost,tonic,并解决 prost0.11默认不安装protoc编译器的问题
- 运维能力： 删除一键安装脚本
- docker镜像： 修改文件存放位置 /root，从而兼容unraid文件无法映射的问题（也许可以）

## TODO
- [x] 增加 udeps 检查 github action
- [x] 请求数据状态支持跨域，最后可能会和 **HomeCenter** 集成到一起
- [ ] 安全传输 tls
- [ ] 自定义数据采集间隔
- [ ] 不依赖 linux native 方式收集数据，从而支持更多架构的设备，比如路由器



## 感谢
[ServerStatus-Rust](https://github.com/zdz/ServerStatus-Rust)
[Rust语言圣经](https://github.com/sunface/rust-course)


