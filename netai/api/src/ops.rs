use ipis::core::ndarray;

pub fn argmax<S, D>(mat: &ndarray::ArrayBase<S, D>) -> ndarray::Array1<usize>
where
    S: ndarray::Data,
    S::Elem: PartialOrd,
    D: ndarray::Dimension,
{
    mat.rows()
        .into_iter()
        .map(|row| {
            row.into_iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .unwrap()
                .0
        })
        .collect()
}
