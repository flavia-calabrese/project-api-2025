# To start container

```shell
cd nome_cartella_progetto

# per buildare l'immagine
docker build -t rust-fuse-dev .

# per runnare il container
docker run -it \
    --privileged \
    --cap-add=SYS_PTRACE \
    --device /dev/fuse \
    -v $(pwd):/workspace \
    --name remote_fs_container \
    rust-fuse-dev
```
