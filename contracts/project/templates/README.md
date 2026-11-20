# Project templates

One file per template. A template is data: `template_id`, `name`, `description`, `modes` (the
tray modes this template serves: `breaking_news`, `story`, `net`; at most one template per mode)
and `settings`, the project settings a new project starts from. `storage.projects_root` is set by
the block that makes the project; what a template leaves out, the defaults block completes.
