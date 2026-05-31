import { useEffect, useState } from 'react'
import {
  useReactTable,
  getCoreRowModel,
  flexRender,
  createColumnHelper,
} from '@tanstack/react-table'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { StatusBadge } from '@/components/status-badge'
import { api } from '@/api/client'
import type { Verdict } from '@/types'

const columnHelper = createColumnHelper<Verdict>()

const columns = [
  columnHelper.accessor('id', {
    header: 'ID',
    cell: (info) => <span className="font-mono text-xs">{info.getValue().slice(0, 8)}...</span>,
  }),
  columnHelper.accessor('action_id', {
    header: 'Action ID',
    cell: (info) => <span className="font-mono text-xs">{info.getValue().slice(0, 8)}...</span>,
  }),
  columnHelper.accessor('decision', {
    header: 'Decision',
    cell: (info) => <StatusBadge status={info.getValue()} />,
  }),
  columnHelper.accessor('risk_level', {
    header: 'Risk Level',
  }),
  columnHelper.accessor('confidence', {
    header: 'Confidence',
    cell: (info) => {
      const val = info.getValue()
      return val != null ? `${(val * 100).toFixed(0)}%` : '-'
    },
  }),
  columnHelper.accessor('reason', {
    header: 'Reason',
    cell: (info) => (
      <span className="max-w-[300px] truncate block">{info.getValue()}</span>
    ),
  }),
  columnHelper.accessor('policy_matched', {
    header: 'Policy',
    cell: (info) => info.getValue() ?? '-',
  }),
  columnHelper.accessor('created_at', {
    header: 'Created',
    cell: (info) => new Date(info.getValue()).toLocaleString(),
  }),
]

export default function Verdicts() {
  const [data, setData] = useState<Verdict[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    api.listVerdicts()
      .then(setData)
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [])

  const table = useReactTable({
    data,
    columns,
    getCoreRowModel: getCoreRowModel(),
  })

  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Verdicts</h1>
        <p className="text-sm text-muted-foreground">LLM decision history</p>
      </div>

      <div className="rounded-md border">
          <Table>
            <TableHeader>
              {table.getHeaderGroups().map((group) => (
                <TableRow key={group.id}>
                  {group.headers.map((header) => (
                    <TableHead key={header.id}>
                      {flexRender(header.column.columnDef.header, header.getContext())}
                    </TableHead>
                  ))}
                </TableRow>
              ))}
            </TableHeader>
            <TableBody>
              {loading ? (
                <TableRow>
                  <TableCell colSpan={columns.length} className="h-24 text-center">Loading...</TableCell>
                </TableRow>
              ) : table.getRowModel().rows.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={columns.length} className="h-24 text-center text-muted-foreground">
                    No verdicts yet
                  </TableCell>
                </TableRow>
              ) : (
                table.getRowModel().rows.map((row) => (
                  <TableRow key={row.id}>
                    {row.getVisibleCells().map((cell) => (
                      <TableCell key={cell.id}>
                        {flexRender(cell.column.columnDef.cell, cell.getContext())}
                      </TableCell>
                    ))}
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
      </div>
    </div>
  )
}
