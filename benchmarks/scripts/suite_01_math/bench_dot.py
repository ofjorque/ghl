import numpy as np

np.random.seed(42)
a = np.random.rand(10_000_000)
np.random.seed(43)
b = np.random.rand(10_000_000)
res = np.dot(a, b)
print(res)
