import { useEffect, useState, useMemo, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import {
  useReactTable,
  getCoreRowModel,
  getPaginationRowModel,
  flexRender,
  createColumnHelper,
} from '@tanstack/react-table'
import { ArrowUpDown, Download, ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
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
import type { Action } from '@/types'

const columnHelper = createColumnHelper<Action>()

const columns = [
  columnHelper.accessor('id', {
    header: 'ID',
    cell: (info) => <span className="font-mono text-xs">{info.getValue().slice(0, 8)}...</span>,
  }),
  columnHelper.accessor('agent_name', {
    header: ({ column }) => (
      <Button variant="ghost" size="sm" onClick={() => column.toggleSorting()}>
        Agent <ArrowUpDown className="ml-1 h-3 w-3" />
      </Button>
    ),
  }),
  columnHelper.accessor('intent', {
    header: 'Intent',
    cell: (info) => (
      <span className="max-w-[200px] truncate block">{info.getValue()}</span>
    ),
  }),
  columnHelper.accessor('status', {
    header: 'Status',
    cell: (info) => <StatusBadge status={info.getValue()} />,
  }),
  columnHelper.accessor('risk_level', {
    header: 'Risk',
    cell: (info) => info.getValue() ?? '-',
  }),
  columnHelper.accessor('created_at', {
    header: 'Created',
    cell: (info) => new Date(info.getValue()).toLocaleString(),
  }),
]

function useDebounce<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value)
  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), delay)
    return () => clearTimeout(id)
  }, [value, delay])
  return debounced
}

export default function ActionsPage() {
  const [data, setData] = useState<Action[]>([])
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')
  const debouncedSearch = useDebounce(search, 300)
  const navigate = useNavigate()

  useEffect(() => {
    api.listActions()
      .then(setData)
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [])

  const filtered = useMemo(
    () => data.filter(
      (a) =>
        a.agent_name.toLowerCase().includes(debouncedSearch.toLowerCase()) ||
        a.intent.toLowerCase().includes(debouncedSearch.toLowerCase()) ||
        a.id.includes(debouncedSearch)
    ),
    [data, debouncedSearch]
  )

  const table = useReactTable({
    data: filtered,
    columns,
    getCoreRowModel: getCoreRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    initialState: { pagination: { pageSize: 20 } },
  })

  const handleRowClick = useCallback((id: string) => {
    navigate(`/admin/actions/${id}`)
  }, [navigate])

  if (loading) {
    return (
      <div className="space-y-4">
        <h1 className="text-2xl font-semibold tracking-tight">Actions</h1>
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
              <TableRow>
                <TableCell colSpan={columns.length} className="h-24 text-center">
                  Loading...
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Actions</h1>
          <p className="text-sm text-muted-foreground">All agent actions and their verdicts</p>
        </div>
        <Button variant="outline" size="sm" onClick={() => api.exportActions()}>
          <Download className="mr-1 h-4 w-4" /> Export CSV
        </Button>
      </div>

      <div className="flex items-center gap-2">
        <Input
          placeholder="Search actions..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="max-w-sm"
        />
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
            {table.getRowModel().rows.length === 0 ? (
              <TableRow>
                <TableCell colSpan={columns.length} className="h-24 text-center text-muted-foreground">
                  No actions found
                </TableCell>
              </TableRow>
            ) : (
              table.getRowModel().rows.map((row) => (
                <TableRow
                  key={row.id}
                  className="cursor-pointer"
                  onClick={() => handleRowClick(row.original.id)}
                >
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

      <div className="flex items-center justify-between text-sm text-muted-foreground">
        <div>
          Page {table.getState().pagination.pageIndex + 1} of {table.getPageCount()} ({filtered.length} total)
        </div>
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="sm" onClick={() => table.setPageIndex(0)} disabled={!table.getCanPreviousPage()}>
            <ChevronsLeft className="h-4 w-4" />
          </Button>
          <Button variant="ghost" size="sm" onClick={() => table.previousPage()} disabled={!table.getCanPreviousPage()}>
            <ChevronLeft className="h-4 w-4" />
          </Button>
          <Button variant="ghost" size="sm" onClick={() => table.nextPage()} disabled={!table.getCanNextPage()}>
            <ChevronRight className="h-4 w-4" />
          </Button>
          <Button variant="ghost" size="sm" onClick={() => table.setPageIndex(table.getPageCount() - 1)} disabled={!table.getCanNextPage()}>
            <ChevronsRight className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  )
}
