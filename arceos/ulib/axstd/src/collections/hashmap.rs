//! A minimal `HashMap` for ArceOS's `axstd`.
//!
//! `std::collections::HashMap` 不在 `alloc` 里，因为它的默认哈希器
//! `RandomState` 需要操作系统提供随机种子。这里我们用确定性的
//! FNV-1a 哈希 + 开放寻址（线性探测）自己实现一个。

use alloc::vec::Vec;
use core::hash::{Hash, Hasher};
use core::mem;

// ============ 1. FNV-1a 哈希器 ============

#[derive(Clone)]
struct FnvHasher(u64);

impl FnvHasher {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }
}

impl Hasher for FnvHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    // Hasher 的 write_u8/write_u32 等方法都有默认实现，
    // 它们最终都会转发到 write()，所以我们只需要实现 write。
    fn write(&mut self, bytes: &[u8]) {
        let mut hash = self.0;
        for &b in bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(Self::PRIME); // wrapping 防溢出 panic
        }
        self.0 = hash;
    }
}

fn hash_key<K: Hash>(key: &K) -> usize {
    let mut h = FnvHasher::new();
    key.hash(&mut h);   // String 的 Hash 会写入字节内容 + 0xff 结尾标记
    h.finish() as usize
}

// ============ 2. 桶条目 ============

enum Entry<K, V> {
    Empty,
    Tombstone,
    Occupied(K, V),
}

// ============ 3. HashMap 本体 ============

pub struct HashMap<K, V> {
    buckets: Vec<Entry<K, V>>, // 桶数组，长度永远是 2 的幂
    len: usize,                // Occupied 的个数
    used: usize,               // 非 Empty 的槽位数（Occupied + Tombstone）
}

impl<K, V> HashMap<K, V> {
    /// const fn：编译期就能调用，和 std::collections::HashMap::new() 一样。
    pub const fn new() -> Self {
        Self {
            buckets: Vec::new(),
            len: 0,
            used: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.buckets.clear();
        self.len = 0;
        self.used = 0;
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            inner: self.buckets.iter(),
        }
    }
}

// ============ 4. 需要哈希/比较的操作 ============

impl<K: Eq + Hash, V> HashMap<K, V> {
    /// 定位 key：返回 (已存在的槽位, 可插入的槽位)
    /// 找不到时第一个返回 None；插入槽位优先复用最早的墓碑。
    fn locate(&self, key: &K) -> (Option<usize>, usize) {
        let mask = self.buckets.len() - 1; // 容量是 2 的幂，& mask 等价于 % capacity
        let mut idx = hash_key(key) & mask;
        let mut first_tombstone = None;

        loop {
            match &self.buckets[idx] {
                Entry::Empty => return (None, first_tombstone.unwrap_or(idx)),
                Entry::Tombstone => {
                    if first_tombstone.is_none() {
                        first_tombstone = Some(idx);
                    }
                }
                Entry::Occupied(k, _) if k == key => return (Some(idx), idx),
                _ => {}
            }
            idx = (idx + 1) & mask; // 线性探测：往后挪一格
        }
    }

        /// 重新散列到 new_cap 个新桶。扩容和"清墓碑"都靠它。
    fn rehash(&mut self, new_cap: usize) {
        let mut new_buckets = Vec::new();
        new_buckets.resize_with(new_cap, || Entry::Empty);

        let old = mem::replace(&mut self.buckets, new_buckets);
        self.used = 0;

        for entry in old {
            if let Entry::Occupied(k, v) = entry {
                let mask = self.buckets.len() - 1;
                let mut idx = hash_key(&k) & mask;
                while !matches!(self.buckets[idx], Entry::Empty) {
                    idx = (idx + 1) & mask;
                }
                self.buckets[idx] = Entry::Occupied(k, v);
                self.used += 1;
            }
        }
        // len 不用动：Occupied 的个数没变，Tombstone 全部被丢弃了。
    }

        pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        // 装载因子超过 75% 就扩容；墓碑太多时原地重建（容量不变）。
        if self.buckets.is_empty()
            || (self.used + 1) * 4 > self.buckets.len() * 3
        {
            let cap = self.buckets.len();
            let new_cap = if self.buckets.is_empty() {
                8                                  // 第一次插入：从 8 开始
            } else if self.len * 2 >= self.used {
                cap * 2                             // 数据多 → 翻倍
            } else {
                cap                                  // 墓碑多 → 同容量重建，顺便清墓碑
            };
            self.rehash(new_cap);
        }

        let (found, slot) = self.locate(&key);
        match found {
            Some(i) => {
                // key 已存在：替换 value，返回旧值（和 std 行为一致）
                if let Entry::Occupied(_, v) = &mut self.buckets[i] {
                    Some(mem::replace(v, value))
                } else {
                    unreachable!()
                }
            }
            None => {
                // 统计簿记：Empty → used+1；复用 Tombstone → used 不变（它本来就非空）
                if matches!(self.buckets[slot], Entry::Empty) {
                    self.used += 1;
                }
                self.buckets[slot] = Entry::Occupied(key, value);
                self.len += 1;
                None
            }
        }
    }

        pub fn get(&self, key: &K) -> Option<&V> {
        if self.buckets.is_empty() {
            return None;
        }
        let (found, _) = self.locate(key);
        match found {
            Some(i) => {
                if let Entry::Occupied(_, v) = &self.buckets[i] {
                    Some(v)
                } else {
                    unreachable!()
                }
            }
            None => None,
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if self.buckets.is_empty() {
            return None;
        }
        let (found, _) = self.locate(key);
        match found {
            Some(i) => {
                let old = mem::replace(&mut self.buckets[i], Entry::Tombstone);
                self.len -= 1;
                // used 不变：Occupied 变 Tombstone，仍然是"非 Empty"
                match old {
                    Entry::Occupied(_, v) => Some(v),
                    _ => unreachable!(),
                }
            }
            None => None,
        }
    }
}

// ============ 5. 迭代器 ============

pub struct Iter<'a, K, V> {
    inner: core::slice::Iter<'a, Entry<K, V>>,
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        // 逐个扫过桶，跳过 Empty 和 Tombstone，只交出 Occupied 的 (&K, &V)
        for entry in self.inner.by_ref() {
            if let Entry::Occupied(k, v) = entry {
                return Some((k, v));
            }
        }
        None
    }
}

impl<K, V> Default for HashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}