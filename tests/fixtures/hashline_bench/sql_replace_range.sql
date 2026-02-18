SELECT
  id,
  amount
FROM invoices
WHERE amount > 100
ORDER BY amount DESC;
