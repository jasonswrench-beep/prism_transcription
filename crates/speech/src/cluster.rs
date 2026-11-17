//! Agglomerative clustering of speaker embeddings: average linkage on cosine distance.
//!
//! Built with the nearest-neighbour chain, which finds every merge in O(n²) time and one n×n distance matrix, so
//! a 60-minute recording (a few thousand windows) clusters in well under a second. All merges are recorded and
//! then replayed in order of distance, which gives the same tree as the textbook algorithm because average linkage
//! never makes a later merge closer than an earlier one.

/// How to cut the tree: at a fixed number of clusters (the user's speaker count), or where the next merge would be
/// further apart than a cosine distance.
#[derive(Clone, Copy, Debug)]
pub enum Cut {
    Clusters(usize),
    Distance(f32),
}

fn normalise(v: &[f32]) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    v.iter().map(|x| x / n).collect()
}

/// Cluster `embeddings` and return one label per embedding, numbered 0.. in order of first appearance.
pub fn agglomerative(embeddings: &[Vec<f32>], cut: Cut) -> Vec<u32> {
    let n = embeddings.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![0];
    }
    let e: Vec<Vec<f32>> = embeddings.iter().map(|v| normalise(v)).collect();
    let mut d = vec![0f32; n * n];
    for i in 0..n {
        for j in (i + 1)..n {
            let dot: f32 = e[i].iter().zip(&e[j]).map(|(a, b)| a * b).sum();
            let dist = 1.0 - dot;
            d[i * n + j] = dist;
            d[j * n + i] = dist;
        }
    }
    let mut size = vec![1usize; n];
    let mut active = vec![true; n];
    let mut merges: Vec<(f32, usize, usize)> = Vec::with_capacity(n - 1);
    let mut chain: Vec<usize> = Vec::new();
    let mut remaining = n;
    while remaining > 1 {
        if chain.is_empty() {
            chain.push((0..n).find(|&i| active[i]).unwrap());
        }
        let a = *chain.last().unwrap();
        let prev = if chain.len() >= 2 { Some(chain[chain.len() - 2]) } else { None };
        // Nearest active neighbour of a; ties go to the one already behind it in the chain.
        let mut best = usize::MAX;
        let mut best_d = f32::INFINITY;
        for j in 0..n {
            if j != a && active[j] {
                let dj = d[a * n + j];
                if dj < best_d || (dj == best_d && Some(j) == prev) {
                    best_d = dj;
                    best = j;
                }
            }
        }
        if Some(best) == prev {
            chain.pop();
            chain.pop();
            let (x, y) = (a.min(best), a.max(best));
            merges.push((best_d, x, y));
            // Lance-Williams update for average linkage; the merged cluster keeps index x.
            let (sx, sy) = (size[x] as f32, size[y] as f32);
            for k in 0..n {
                if active[k] && k != x && k != y {
                    let v = (sx * d[x * n + k] + sy * d[y * n + k]) / (sx + sy);
                    d[x * n + k] = v;
                    d[k * n + x] = v;
                }
            }
            size[x] += size[y];
            active[y] = false;
            remaining -= 1;
        } else {
            chain.push(best);
        }
    }
    // Replay the merges from closest to furthest with a union-find, stopping at the cut.
    merges.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let mut clusters = n;
    for (dist, x, y) in merges {
        let stop = match cut {
            Cut::Clusters(k) => clusters <= k.max(1),
            Cut::Distance(t) => dist > t,
        };
        if stop {
            break;
        }
        let (rx, ry) = (find(&mut parent, x), find(&mut parent, y));
        if rx != ry {
            parent[ry] = rx;
            clusters -= 1;
        }
    }
    let mut label_of_root = std::collections::HashMap::new();
    (0..n)
        .map(|i| {
            let r = find(&mut parent, i);
            let next = label_of_root.len() as u32;
            *label_of_root.entry(r).or_insert(next)
        })
        .collect()
}

fn centroid(e: &[Vec<f32>], members: impl Iterator<Item = usize>) -> Vec<f32> {
    let mut c = vec![0f32; e[0].len()];
    for i in members {
        for (a, b) in c.iter_mut().zip(&e[i]) {
            *a += b;
        }
    }
    normalise(&c)
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Speaker clustering that is robust to odd windows: [`agglomerative`], then any cluster smaller than
/// `max(2, 1 % of windows)` is treated as noise (a cough, a laugh, a half word) rather than a speaker, and the
/// remaining clusters are refined by moving every window to its nearest cluster centre (three rounds).
///
/// With [`Cut::Clusters`]`(k)` the tree is cut further down until it holds `k` clusters that are not noise, so a
/// requested speaker count is never used up by outliers.
pub fn cluster_speakers(embeddings: &[Vec<f32>], cut: Cut) -> Vec<u32> {
    let n = embeddings.len();
    if n < 2 {
        return vec![0; n];
    }
    let e: Vec<Vec<f32>> = embeddings.iter().map(|v| normalise(v)).collect();
    let min_size = (n / 100).max(2);
    let big = |labels: &[u32]| -> Vec<u32> {
        let mut count = std::collections::HashMap::new();
        for &l in labels {
            *count.entry(l).or_insert(0usize) += 1;
        }
        let mut b: Vec<u32> = count.into_iter().filter(|&(_, c)| c >= min_size).map(|(l, _)| l).collect();
        b.sort();
        b
    };
    let mut labels = agglomerative(&e, cut);
    if let Cut::Clusters(k) = cut {
        let mut m = k;
        while big(&labels).len() < k && m < n {
            m += 1;
            labels = agglomerative(&e, Cut::Clusters(m));
        }
    }
    let keep = big(&labels);
    if keep.is_empty() {
        return vec![0; n];
    }
    let mut centres: Vec<Vec<f32>> = keep.iter().map(|&l| centroid(&e, (0..n).filter(|&i| labels[i] == l))).collect();
    if let Cut::Clusters(k) = cut {
        // Keep the k largest if the deeper cut produced more.
        let mut sizes: Vec<(usize, usize)> = keep.iter().enumerate().map(|(ci, &l)| (labels.iter().filter(|&&x| x == l).count(), ci)).collect();
        sizes.sort_by(|a, b| b.0.cmp(&a.0));
        centres = sizes.iter().take(k).map(|&(_, ci)| centres[ci].clone()).collect();
    }
    let mut assign = vec![0usize; n];
    for _ in 0..3 {
        for i in 0..n {
            assign[i] = (0..centres.len()).max_by(|&a, &b| dot(&e[i], &centres[a]).total_cmp(&dot(&e[i], &centres[b]))).unwrap();
        }
        for (c, centre) in centres.iter_mut().enumerate() {
            if assign.contains(&c) {
                *centre = centroid(&e, (0..n).filter(|&i| assign[i] == c));
            }
        }
    }
    // Number labels in order of first appearance.
    let mut seen: Vec<usize> = Vec::new();
    assign
        .into_iter()
        .map(|c| match seen.iter().position(|&x| x == c) {
            Some(p) => p as u32,
            None => {
                seen.push(c);
                (seen.len() - 1) as u32
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outliers_do_not_use_up_the_speaker_count() {
        // Two speakers of ten windows each and one odd window: asked for two speakers, plain average linkage
        // splits off the odd window; cluster_speakers finds the two speakers.
        let mut e: Vec<Vec<f32>> = Vec::new();
        for i in 0..10 {
            e.push(vec![1.0, 0.02 * i as f32, 0.0]);
            e.push(vec![0.0, 1.0, 0.02 * i as f32]);
        }
        e.push(vec![-1.0, -1.0, -1.0]);
        let plain = agglomerative(&e, Cut::Clusters(2));
        assert_eq!(plain.iter().filter(|&&l| l == plain[20]).count(), 1, "plain linkage isolates the outlier");
        let l = cluster_speakers(&e, Cut::Clusters(2));
        assert!((0..20).step_by(2).all(|i| l[i] == l[0]));
        assert!((1..20).step_by(2).all(|i| l[i] == l[1]));
        assert_ne!(l[0], l[1]);
    }

    fn v(x: f32, y: f32) -> Vec<f32> {
        vec![x, y, 0.0]
    }

    #[test]
    fn two_clear_groups() {
        let e = [v(1.0, 0.05), v(1.0, -0.02), v(0.02, 1.0), v(1.0, 0.0), v(-0.03, 1.0)];
        assert_eq!(agglomerative(&e, Cut::Distance(0.3)), [0, 0, 1, 0, 1]);
        assert_eq!(agglomerative(&e, Cut::Clusters(2)), [0, 0, 1, 0, 1]);
        assert_eq!(agglomerative(&e, Cut::Clusters(1)), [0, 0, 0, 0, 0]);
        assert_eq!(agglomerative(&e, Cut::Clusters(9)), [0, 1, 2, 3, 4]);
    }

    #[test]
    fn matches_naive_average_linkage() {
        // A small pseudo-random set, checked against the textbook O(n^3) algorithm at every cut.
        let mut seed = 12345u32;
        let mut r = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / (1 << 24) as f32 - 0.5
        };
        let e: Vec<Vec<f32>> = (0..24).map(|_| (0..6).map(|_| r()).collect()).collect();
        for k in 1..=24 {
            let fast = agglomerative(&e, Cut::Clusters(k));
            let naive = naive(&e, k);
            assert_eq!(canon(&fast), canon(&naive), "k = {k}");
        }
    }

    fn canon(l: &[u32]) -> Vec<usize> {
        // Each item's label as the index of the first item with that label.
        l.iter().map(|x| l.iter().position(|y| y == x).unwrap()).collect()
    }

    fn naive(e: &[Vec<f32>], k: usize) -> Vec<u32> {
        let e: Vec<Vec<f32>> = e.iter().map(|v| normalise(v)).collect();
        let dist = |a: &Vec<usize>, b: &Vec<usize>| {
            let mut s = 0.0;
            for &i in a {
                for &j in b {
                    s += 1.0 - e[i].iter().zip(&e[j]).map(|(x, y)| x * y).sum::<f32>();
                }
            }
            s / (a.len() * b.len()) as f32
        };
        let mut cl: Vec<Vec<usize>> = (0..e.len()).map(|i| vec![i]).collect();
        while cl.len() > k {
            let (mut bi, mut bj, mut bd) = (0, 1, f32::INFINITY);
            for i in 0..cl.len() {
                for j in (i + 1)..cl.len() {
                    let dd = dist(&cl[i], &cl[j]);
                    if dd < bd {
                        (bi, bj, bd) = (i, j, dd);
                    }
                }
            }
            let b = cl.remove(bj);
            cl[bi].extend(b);
        }
        let mut lab = vec![0u32; e.len()];
        for (c, members) in cl.iter().enumerate() {
            for &m in members {
                lab[m] = c as u32;
            }
        }
        lab
    }
}
