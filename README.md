# x86-qemu-fuzzer

## 框架

- 

## 运行

1. 安装 LLVM（以 15 为例）：

    ```bash
    sudo apt-get install llvm-15 llvm-15-dev
    ```

2. 安装 [cargo-make](https://github.com/sagiegurari/cargo-make)：

    ```bash
    cargo install cargo-make
    ```

3. 修改 `Cargo.toml`：

    1. 关于 LibAFL：LibAFL 更新极快。若使用 LibAFL 原仓库，移除 `[patch."https://github.com/AFLplusplus/LibAFL/"]` 部分；若使用本地的 LibAFL，将 `[patch."https://github.com/AFLplusplus/LibAFL/"]` 中的 `path` 修改为实际路径。

    2. 默认使用 x86_64。若使用 32 位架构（如 `kernels/xv6-public`），需要将 `libafl_qemu` 的 `features` 中的 `x86_64` 修改为 `i386`。

4. 修改内核：若添加了新的内核，需要添加相应的 harness，可参考 `kernel/xv6-x86_64` 做相应修改。

5. 修改 `Makefile.toml`：

    1. 修改 `KERNEL_NAME`、`KERNEL_DIR` 分别为内核的名称和路径。
   
    2. 修改 `LLVM_CONFIG` 为 LLVM 的实际版本号。

    3. 若添加了新的内核，需要添加相应的运行指令：

        ```toml
        [tasks.run_fuzzer_<name>]
        command = "${TARGET_DIR}/${PROFILE}/x86-qemu-fuzzer"
        args = [
            "--",
            <args...>
        ]
        ```
        
        也可根据需要修改编译指令，默认的编译指令是：

        ```toml
        [tasks.target]
        dependencies = ["target_dir"]
        script_runner = "@shell"
        script = """
        make -C ${KERNEL_DIR} CFLAGS_EXTRA="-D ${TARGET_DEFINE} -I${TARGET_DIR}/${PROFILE}/include"
        """
        ```

## 参考资料

- LibAFL book：https://aflplus.plus/libafl-book/libafl.html

- LibAFL paper：https://dl.acm.org/doi/abs/10.1145/3548606.3560602

- LibAFL QEMU paper：https://hal.science/hal-04500872/