## Install

On macOS, first make sure Xcode Command Line Tools are installed by running:

```shell
xcode-select --install
```

Then, on any system:

```shell
curl -fLo ~/.yadm.tmp https://github.com/TheLocehiliosan/yadm/raw/master/yadm && \
chmod a+x ~/.yadm.tmp && \
~/.yadm.tmp clone --no-bootstrap https://github.com/no-simpler/reliquary.git && \
~/.yadm.tmp restore --staged $HOME && \
~/.yadm.tmp checkout -- $HOME && \
~/.yadm.tmp bootstrap && \
rm -f ~/.yadm.tmp
```

## Next steps

- Restart shell.
- Run `yadm decrypt`, then `yadm bootstrap` again.
- Look through `pb` output.

## Update

```shell
yadm update
# or equavalently
yadm pull --ff-only
```
