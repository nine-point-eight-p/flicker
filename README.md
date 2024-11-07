# flicker

一个简单、通用的内核模糊测试工具。

本项目是 2024 全国大学生系统能力大赛操作系统设计赛 OS 功能挑战赛道（以下简称“比赛”）的[参赛作品](https://gitlab.eduxiji.net/T202410003993297/project2210132-239820/-/tree/main)的组成部分。

## 简介

flicker 是一个基于 [LibAFL](https://github.com/AFLplusplus/LibAFL) 和 [LibAFL QEMU](https://github.com/AFLplusplus/LibAFL/tree/main/libafl_qemu) 的内核模糊测试工具，能够帮助用户检测内核中的潜在漏洞。flicker 主要由两部分构成：

1. fuzzer：运行在 host 机器上，负责生成包含若干系统调用的测例，传递给 harness 执行，监控崩溃、超时等异常状况。执行结束后，分析执行过程的覆盖率、所需时间等信息，由此进一步改变测例的参数，以期提高目标程序的覆盖率。

2. harness：运行在 QEMU 的 guest OS 上，负责接受前者生成的测例，解析后发出系统调用。

flicker 的主要特点有：

1. 简单易用：只需描述系统调用、编写简单的 harness、配置运行方式，即可开始测试。

2. 通用：支持任何可由 [syzlang](https://github.com/google/syzkaller/blob/master/docs/syscall_descriptions.md) 描述的系统调用接口，以及任何底层框架支持的架构（包括 x86、ARM、RISC-V 等）。

3. 高效：参考 [syzkaller](https://github.com/google/syzkaller/tree/master) 设计具有类型信息的结构化测例，并实现相应的生成和变异（mutation）算法，同时充分利用 LibAFL 和 LibAFL QEMU 提供的 API 实现测试流程，提高测试效率。

关于更详细的结构设计、技术细节等信息，请参考[比赛文档](https://gitlab.eduxiji.net/T202410003993297/project2210132-239820/-/blob/main/Final-2nd.md#flicker%E5%9F%BA%E4%BA%8E-libafl-%E7%9A%84%E6%A8%A1%E7%B3%8A%E6%B5%8B%E8%AF%95%E5%B7%A5%E5%85%B7)。

## 项目结构

### 源代码

源代码位于 `src/` 目录下，主要包含 2 个部分：

- 库（`lib.rs`）：负责生成/变异测试用例。提供了面向 LibAFL 的组件 `SyscallInput`、`SyscallGenerator` 和 4 种 `SyscallMutator`，分别是系统调用测试用例的载体、系统调用测试用例的生成器和变异器。其具体实现又由以下几个部分组成：

    - `Syscall`：表示系统调用信息的结构体，每个 `Syscall` 包含若干表示参数字段的 `Field`，不同类型的 `Field` 实现了 `GenerateArg` 和 `MutateArg` 两个 trait，能够根据自身信息生成参数或变异特定参数。相关代码主要在 `program/syscall` 目录下。

    - `Call`：表示生成的系统调用，包含了具体数据，去除了冗余的类型信息。每个 `Call` 包含若干 `Arg`，不同类型的 `Arg` 实现了 `ToExecByte` trait，能够将数据序列化成 harness 可以解析的序列。相关代码主要在 `program/call.rs` 中。

    - `Context`：记录生成/变异过程中产生的信息。相关代码主要在 `program/context.rs` 中。

- 可执行程序（`main.rs`）：负责具体的测试流程。其工作流程如下：

    - 初始化：读取 syzlang 系统调用描述文件和常数文件，形成 `Syscall` 结构体。

    - 生成测例：`Generator` 生成初始测例。

    - 执行：从测例库（corpus）中选取测例，运行测试，监控状态。

    - 优化测例：根据覆盖率、运行时间等反馈信息判断当前测例的价值，对测例进行变异。

### 其它

- `corpus/`：有价值的测例，其中 `corpus/init` 存放用户设置的初始化测试用例，`corpus/gen` 存放生成的测试用例。

- `crashes/`：能够产生异常的测例（在 LibAFL 中也被称为 solution）。

- `desc/`：syzlang 描述的系统调用信息。

    - `desc/builtin.txt`：syzlang 内置类型、模板等。

    - `desc/comp.txt`：比赛常用的系统调用描述，摘自 [syzkaller](https://github.com/google/syzkaller/blob/master/sys/linux/sys.txt)。

    - `desc/sys.txt.const`：Linux 系统调用的相关常数，摘自 [syzkaller](https://github.com/google/syzkaller/blob/master/sys/linux/sys.txt.const)。

    - `desc/test.txt`：测试用系统调用描述。

    - `desc/test.txt.const`：测试用系统调用的相关常数。

- `kernel/`：内核源码。建议每个内核单独建立一个目录（如 `kernel/rCore-Tutorial-v3`），方便管理。

- `makefiles/`：配置如何运行 fuzzer 的 Makefile，例如准备工作、启动参数等，参见[添加内核](#添加内核)一节。建议每个内核单独编写一个 Makefile（如 `makefiles/rCore-Tutorial-v3.toml`），方便管理。

- `target/`：编译生成的文件。

## 如何使用

### 准备环境

1. 安装 LLVM（以 15 为例，请参考 [LibAFL 推荐的 LLVM 版本](https://github.com/AFLplusplus/LibAFL#building-and-installing)）：

    ```bash
    sudo apt-get install llvm-15 llvm-15-dev
    ```

2. 安装 [cargo-make](https://github.com/sagiegurari/cargo-make)：

    ```bash
    cargo install cargo-make
    ```

### 添加内核

1. 使用 [syzlang](https://github.com/google/syzkaller/blob/master/docs/syscall_descriptions.md) 描述待测的系统调用。可参考 syzkaller [关于 syzlang 的说明文档](https://github.com/google/syzkaller/blob/master/docs/syscall_descriptions_syntax.md)及 syzkaller 仓库中的系统调用描述文件，也可参考 `desc/` 目录下的描述文件。

2. 编写 harness。

    1. 测例解析与执行：fuzzer 与 harness 之间存在一套序列化/反序列化测例的协议，可参考 `src/program/call.rs` 中 `ToExecBytes` 的实现。对于不关心具体细节的用户，建议使用本项目的辅助工具 [syscall2struct](https://github.com/nine-point-eight-p/syscall2struct) 将描述文件中的每个待测系统调用转换为一个 Rust 结构体，通过 serde 的 `Deserialize`、`Serialize` trait 实现系统调用数据的序列化/反序列化，并通过该库的 `MakeSyscall` 或 `MakeSyscallMut` trait 实现执行系统调用的逻辑。

    2. 实现 harness：为内核添加一个用户程序，根据 [LibAFL QEMU 的接口](https://github.com/AFLplusplus/LibAFL/blob/main/libafl_qemu/runtime/libafl_qemu.h)，首先调用 start 命令，之后从缓冲区依次读取测例、解析、执行，最后调用 end 命令。可参考已有示例实现。其中，解析过程的具体实现需要 [postcard](https://docs.rs/postcard/1.0.10/postcard/) 的支持；对于 Rust 编写的内核，[libafl_qemu_cmd](https://github.com/nine-point-eight-p/libafl_qemu_cmd) 提供了 LibAFL QEMU 接口的 Rust 版本。

3. 使用基于 [cargo-make](https://github.com/sagiegurari/cargo-make) 的 Makefile 配置运行方法。

    1. 添加 Makefile：在 `makefiles/` 目录下为待测内核新建一个 Makefile，如 `makefiles/rCore-Tutorial-v3.toml`。

    2. 配置 Makefile：`Makefile.toml` 是所有 Makefile 的基础模板，内容包括：

        - 环境变量：编译选项、文件路径、LibAFL 相关配置等。

        - 任务：如下图所示，其中方角矩形是已实现的任务，圆角矩形是未具体实现的任务。

            ```mermaid
            flowchart TD
                R(run) --> B[build]
                B[build] --> B1[build_fuzzer]
                B --> B2(build_kernel)
                C[clean] ---> C1[clean_fuzzer]
                C ---> C2(clean_kernel)
            ```

        因此，新的 Makefile 应至少包括以下内容：

        ```toml
        extending = "../Makefile.toml"

        [env]
        KERNEL_NAME = "..."
        KERNEL_DIR = "..."

        [tasks.build_kernel]
        command = "..."
        args = ["..."]

        [tasks.clean_kernel]
        command = "..."
        args = ["..."]

        [tasks.run]
        command = "..."
        args = ["..."]
        ```

        亦可修改环境变量、已有任务或添加新的任务。可参考 `makefiles/` 目录下的示例。

### 运行

Makefile 配置完成后，可通过 `cargo make` 编译或直接运行 fuzzer，如：

```bash
cargo make --makefile path/to/makefile.toml build
cargo make --makefile path/to/makefile.toml run
```

注意需要先通过 `--makefile` 导入 Makefile，再指定任务，保证环境变量正确加载。

flicker 还提供了测例复现功能，请参考 `makefiles/Alien.toml` 中的 `reproduce` 任务。

## TODO

- [ ] 更新 LibAFL 依赖。

- [ ] 支持更多 syzlang 类型，如 `array`、`struct`、`union` 等。

- [ ] 完善测例生成算法，提高测试效率，如 `resource` 类型的生成、变异。

## 参考资料

- LibAFL book：https://aflplus.plus/libafl-book/libafl.html

- LibAFL paper：https://dl.acm.org/doi/abs/10.1145/3548606.3560602

- LibAFL QEMU paper：https://hal.science/hal-04500872/