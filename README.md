# crashdump_viewer_cli

Getting an ejabberd dump
```
docker run --name ejabberd \
  -it \
  -p 5222:5222 \
  --mount type=bind,source=./,dst=/opt/ejabberd/logs \
  ghcr.io/processone/ejabberd live

```


Running
```
cargo run ./sample_dumps/erl_crash_simple.dump
# or
cargo run ./sample_dumps/erl_crash_20250105-004018.dump
```

# Screenshots
![](./screenshots/general_view.png)
![](./screenshots/process_view.png)
