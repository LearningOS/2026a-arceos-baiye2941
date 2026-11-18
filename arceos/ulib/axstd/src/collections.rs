//! 集合类型：转发 alloc 自带的集合，并补充 ArceOS 自己的 HashMap。

// 先把 alloc::collections 的所有公开项（BTreeMap、VecDeque、BinaryHeap……）
// 原样导出，保证和以前 std::collections 的行为一致。
pub use alloc::collections::*;

mod hashmap;
pub use hashmap::HashMap;