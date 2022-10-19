The ttx files are the masters. To update the binaries:

```shell
# from the repo root
for f in $(ls resources/test_fonts/ttx/*.ttx); do ttx -o ${f/.ttx/.ttf} $f; done
```
