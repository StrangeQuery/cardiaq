<p align="center"><img src="assets/burn.png" alt="heartburn: a rusty sawblade with CARDIAC's instruction clock cut through it" width="256"></p>
<p align="center"><a href="https://lab.strangequery.com/heartburn/"><code>lab.strangequery.com/heartburn</code></a></p>
<p align="center"><a href="https://github.com/StrangeQuery/heartburn"><img src="https://img.shields.io/badge/GitHub-heartburn-181717?logo=github&amp;logoColor=white" alt="GitHub"></a> <a href="https://discord.gg/VsqKEdBYzJ"><img src="https://img.shields.io/badge/Discord-join-5865F2?logo=discord&amp;logoColor=white" alt="Discord"></a></p>

# heartburn

The world's fastest emulator for CARDIAC, the 1968 Bell Labs cardboard computer.

<h3>Note</h3>
This code library is used by the products below. To write CARDIAC programs in Rust, install the compiler with the
command below:

<br/>

`cargo install cargo-cardiac`

This library will be included as part of that install. With the `tools` feature, it also has the programs `burn`,
`carc` and `ekg` (`heartburn::tools`), which `cargo cardiac run`, `debug` and `asm` are built on. To get them as
programs of their own, without Rust, see the [README](https://github.com/StrangeQuery/heartburn#installation).

<table>
<tr>
<td align="center" valign="top" width="33%"><a href="https://crates.io/crates/cardiac"><img src="https://raw.githubusercontent.com/StrangeQuery/rust-cardiac/HEAD/assets/logo.png" alt="cardiac" width="96"></a><br><a href="https://crates.io/crates/cardiac"><b><code>cardiac</code></b></a><br><b>Strange Query's Rust for CARDIAC</b></td>
<td align="center" valign="top" width="33%"><a href="https://github.com/StrangeQuery/heartburn"><img src="https://raw.githubusercontent.com/StrangeQuery/heartburn/HEAD/assets/burn.png" alt="burn, carc and ekg" width="96"></a><br><a href="https://github.com/StrangeQuery/heartburn"><b>burn, carc and ekg</b></a><br>Programs to run, assemble and debug CARDIAC programs</td>
<td align="center" valign="top" width="33%"><a href="https://lab.strangequery.com/cardigan/"><img src="https://lab.strangequery.com/cardigan/banner.webp" alt="Cardigan" width="160"></a><br><a href="https://lab.strangequery.com/cardigan/"><b>Cardigan</b></a><br>A CARDIAC machine in your browser</td>
</tr>
</table>

MIT licensed.
