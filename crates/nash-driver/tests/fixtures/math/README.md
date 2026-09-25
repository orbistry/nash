# Exact math references

Run `python3 reference.py` with Python 3.9 or later. The standard-library
`math` and `fractions.Fraction` results are asserted in the Nash source fixtures
`../base-traits/IntegerMath.nash` and `../base-traits/Rational.nash`.
They include signs, zero, 129-bit and 257-bit square boundaries, exact fractional
arithmetic, rounding and invalid inputs. No compiler CLI is invoked by tests.
