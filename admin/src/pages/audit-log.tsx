import { useEffect, useState } from 'react'
import { Download, Search, ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  useReactTable,
  getCoreRowModel,
  getPaginationRowModel,
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
import { api } from '@/api/client'
import type { AuditLog } from '@/types'

const columnHelper = createColumnHelper<AuditLog>()

const columns = [
  columnHelper.accessor('event_type', {
    header: 'Event Type',
    cell: (info) => info.getValue(),
  }),
  columnHelper.accessor('action_id', {
    header: 'Action ID',
    cell: (info) => info.getValue() ? <span className="font-mono text-xs">{info.getValue()!.slice(0, 8)}...</span> : '-',
  }),
  columnHelper.accessor('details', {
    header: 'Details',
    cell: (info) => {
      const val = info.getValue()
      return val ? <span className="max-w-[300px] truncate block">{JSON.stringify(val)}</span> : '-'
    },
  }),
  columnHelper.accessor('created_at', {
    header: 'Timestamp',
    cell: (info) => new Date(info.getValue()).toLocaleString(),
  }),
]

export default function AuditLog() {
  const [data, setData] = useState<AuditLog[]>([])
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')

  const fetchLogs = async (q?: string) => {
    try {
      const result = q ? await api.searchAuditLog(q) : await api.listAuditLog()
      setData(result)
    } catch {
      // ignore
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { fetchLogs() }, [])

  const handleSearch = () => {
    setLoading(true)
    fetchLogs(search || undefined)
  }

  const table = useReactTable({
    data,
    columns,
    getCoreRowModel: getCoreRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    initialState: { pagination: { pageSize: 20 } },
  })

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Audit Log</h1>
          <p className="text-sm text-muted-foreground">Immutable record of all events</p>
        </div>
        <Button variant="outline" size="sm" onClick={() => api.exportAuditLog()}>
          <Download className="mr-1 h-4 w-4" /> Export
        </Button>
      </div>

      <div className="flex items-center gap-2">
        <div className="relative max-w-sm flex-1">
          <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            placeholder="Search audit log..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleSearch()}
            className="pl-8"
          />
        </div>
        <Button variant="secondary" size="sm" onClick={handleSearch}>
          Search
        </Button>
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
                    No audit log entries
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

      <div className="flex items-center justify-between text-sm text-muted-foreground">
        <div>
          Page {table.getState().pagination.pageIndex + 1} of {table.getPageCount()} ({data.length} total)
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
