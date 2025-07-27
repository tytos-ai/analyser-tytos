'use client'

import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Skeleton } from '@/components/ui/skeleton'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Checkbox } from '@/components/ui/checkbox'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useToast } from '@/hooks/use-toast'
import { Play, Square, RotateCcw, Settings, Save, AlertTriangle, Info } from 'lucide-react'
import React from 'react'
import { api } from '@/lib/api'

// Types matching the actual Rust API responses
interface ServiceState {
  state: string // 'Stopped' | 'Starting' | 'Running' | 'Stopping' | 'Error'
  discovered_wallets_total?: number
  queue_size?: number
  last_cycle_wallets?: number
  cycles_completed?: number
  last_activity?: string | null
  wallets_processed?: number
  wallets_in_progress?: number
  successful_analyses?: number
  failed_analyses?: number
}

interface ServiceStats {
  wallet_discovery: ServiceState
  pnl_analysis: ServiceState
}

interface ServiceConfig {
  enable_wallet_discovery: boolean
  enable_pnl_analysis: boolean
}

interface ServiceControlRequest {
  service: string
  action: 'start' | 'stop' | 'restart'
  config_override?: any
}

export default function Services() {
  const [confirmDialog, setConfirmDialog] = useState<{
    open: boolean
    service: string
    action: string
    onConfirm: () => void
  }>({ open: false, service: '', action: '', onConfirm: () => {} })
  
  const [serviceConfig, setServiceConfig] = useState<ServiceConfig | null>(null)
  const [pendingServiceConfig, setPendingServiceConfig] = useState<ServiceConfig | null>(null)
  const [hasPendingChanges, setHasPendingChanges] = useState(false)
  
  const { toast } = useToast()
  const queryClient = useQueryClient()

  // Fetch service status using existing API client
  const { data: serviceStats, isLoading: servicesLoading } = useQuery({
    queryKey: ['services-status'],
    queryFn: api.services.getStatus,
    refetchInterval: 3000 // Refresh every 3 seconds
  })

  // Fetch service configuration using existing API client
  const { data: currentServiceConfig, isLoading: serviceConfigLoading } = useQuery({
    queryKey: ['service-config'],
    queryFn: api.services.getConfig,
  })

  // Update service config when data changes
  React.useEffect(() => {
    if (currentServiceConfig) {
      setServiceConfig(currentServiceConfig)
      setPendingServiceConfig(currentServiceConfig)
      setHasPendingChanges(false)
    }
  }, [currentServiceConfig])

  // Service control mutation using existing API client
  const serviceControlMutation = useMutation({
    mutationFn: api.services.control,
    onSuccess: (data) => {
      toast({
        title: "Success",
        description: data.message,
      })
      queryClient.invalidateQueries({ queryKey: ['services-status'] })
    },
    onError: (error) => {
      toast({
        title: "Error",
        description: error instanceof Error ? error.message : "Failed to control service",
        variant: "destructive",
      })
    }
  })

  // Service config update mutation using existing API client
  const serviceConfigMutation = useMutation({
    mutationFn: api.services.updateConfig,
    onSuccess: (data) => {
      toast({
        title: "Configuration Saved",
        description: "Service configuration has been updated successfully",
      })
      setHasPendingChanges(false)
      queryClient.invalidateQueries({ queryKey: ['service-config'] })
    },
    onError: (error) => {
      toast({
        title: "Failed to Save",
        description: error instanceof Error ? error.message : "Failed to update service configuration",
        variant: "destructive",
      })
    }
  })


  const getStatusColor = (state: string) => {
    switch (state) {
      case 'Running':
        return 'bg-green-success/20 text-green-success border-green-success/30'
      case 'Stopped':
        return 'bg-gray-500/20 text-gray-400 border-gray-500/30'
      case 'Starting':
        return 'bg-blue-steel/20 text-blue-steel border-blue-steel/30'
      case 'Stopping':
        return 'bg-orange-warning/20 text-orange-warning border-orange-warning/30'
      case 'Error':
        return 'bg-red-500/20 text-red-400 border-red-500/30'
      default:
        return 'bg-gray-500/20 text-gray-400 border-gray-500/30'
    }
  }

  const handleServiceAction = (service: string, action: 'start' | 'stop' | 'restart') => {
    // Check if service is enabled in configuration for start actions
    if (action === 'start' && pendingServiceConfig) {
      const isEnabled = service === 'wallet_discovery' 
        ? pendingServiceConfig.enable_wallet_discovery 
        : pendingServiceConfig.enable_pnl_analysis
      
      if (!isEnabled) {
        toast({
          title: "Service Disabled",
          description: `${service.replace('_', ' ')} service is disabled in configuration. Please enable it first.`,
          variant: "destructive",
        })
        return
      }
    }

    if (action === 'stop' || action === 'restart') {
      setConfirmDialog({
        open: true,
        service,
        action,
        onConfirm: () => {
          serviceControlMutation.mutate({ service, action })
          setConfirmDialog({ open: false, service: '', action: '', onConfirm: () => {} })
        }
      })
    } else {
      serviceControlMutation.mutate({ service, action })
    }
  }

  const startServiceWithDefaults = (serviceName: string) => {
    const request: ServiceControlRequest = {
      action: 'start',
      service: serviceName
      // No config_override = uses system defaults
    }
    
    serviceControlMutation.mutate(request)
  }


  const handleServiceConfigSave = () => {
    if (pendingServiceConfig) {
      serviceConfigMutation.mutate(pendingServiceConfig)
    }
  }
  
  const handleRevertChanges = () => {
    if (serviceConfig) {
      setPendingServiceConfig(serviceConfig)
      setHasPendingChanges(false)
      toast({
        title: "Changes Reverted",
        description: "Pending changes have been discarded",
      })
    }
  }




  const updatePendingServiceConfig = (key: keyof ServiceConfig, value: boolean) => {
    if (!pendingServiceConfig) return
    
    const newConfig = { ...pendingServiceConfig, [key]: value }
    setPendingServiceConfig(newConfig)
    
    // Check if there are pending changes
    const hasChanges = serviceConfig ? (
      newConfig.enable_wallet_discovery !== serviceConfig.enable_wallet_discovery ||
      newConfig.enable_pnl_analysis !== serviceConfig.enable_pnl_analysis
    ) : false
    
    setHasPendingChanges(hasChanges)
  }


  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-3xl font-bold text-white mb-2">Service Control</h1>
        <p className="text-gray-400">Manage backend services and system configuration</p>
      </div>

      {/* Service Status & Controls */}
      <div className="space-y-6">
        <h2 className="text-2xl font-semibold text-white">Service Status & Controls</h2>
        
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          {servicesLoading || !serviceStats ? (
            Array.from({ length: 2 }).map((_, i) => (
              <Card key={i} className="glass-card border-blue-ice/20">
                <CardHeader>
                  <Skeleton className="h-6 w-3/4" />
                  <Skeleton className="h-4 w-1/2" />
                </CardHeader>
                <CardContent>
                  <Skeleton className="h-10 w-full" />
                </CardContent>
              </Card>
            ))
          ) : (
            ['wallet_discovery', 'pnl_analysis'].map((serviceName) => {
              const serviceData = serviceName === 'wallet_discovery' 
                ? serviceStats.wallet_discovery 
                : serviceStats.pnl_analysis
              const isEnabled = serviceName === 'wallet_discovery'
                ? pendingServiceConfig?.enable_wallet_discovery
                : pendingServiceConfig?.enable_pnl_analysis
              
              return (
                <Card key={serviceName} className="glass-card border-blue-ice/20 hover:border-cyan-bright/50 transition-all duration-300">
                  <CardHeader>
                    <div className="flex items-center justify-between">
                      <CardTitle className="text-white capitalize">
                        {serviceName === 'wallet_discovery' ? 'Wallet Discovery' : 'P&L Analysis'} Service
                      </CardTitle>
                      <div className="flex items-center space-x-2">
                        {!isEnabled && (
                          <Badge className="bg-gray-600/20 text-gray-400 border-gray-600/30">
                            Disabled
                          </Badge>
                        )}
                        <Badge className={getStatusColor(serviceData.state)}>
                          {serviceData.state}
                        </Badge>
                      </div>
                    </div>
                    <CardDescription className="text-gray-400">
                      {serviceName === 'wallet_discovery' 
                        ? `Discovers and tracks wallet addresses. Queue: ${serviceData.queue_size || 0}` 
                        : `Calculates profit and loss metrics. Processed: ${serviceData.wallets_processed || 0}`}
                    </CardDescription>
                    {serviceData.last_activity && (
                      <div className="text-sm text-gray-500">
                        Last activity: {new Date(serviceData.last_activity).toLocaleString()}
                      </div>
                    )}
                  </CardHeader>
                  <CardContent>
                    <div className="flex items-center space-x-2">
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={!isEnabled || serviceData.state === 'Running' || serviceControlMutation.isLoading}
                        onClick={() => startServiceWithDefaults(serviceName)}
                        className="border-green-success/50 text-green-success hover:bg-green-success/20 disabled:opacity-50"
                      >
                        <Play className="w-4 h-4 mr-2" />
                        Start
                      </Button>
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={serviceData.state === 'Stopped' || serviceControlMutation.isLoading}
                        onClick={() => handleServiceAction(serviceName, 'stop')}
                        className="border-red-500/50 text-red-400 hover:bg-red-500/20 disabled:opacity-50"
                      >
                        <Square className="w-4 h-4 mr-2" />
                        Stop
                      </Button>
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={!isEnabled || serviceData.state === 'Stopped' || serviceControlMutation.isLoading}
                        onClick={() => handleServiceAction(serviceName, 'restart')}
                        className="border-orange-warning/50 text-orange-warning hover:bg-orange-warning/20 disabled:opacity-50"
                      >
                        <RotateCcw className="w-4 h-4 mr-2" />
                        Restart
                      </Button>
                    </div>
                  </CardContent>
                </Card>
              )
            })
          )}
        </div>
      </div>
        
        {/* P&L Runtime Configuration */}

      {/* Danger Zone - Service Management */}
      <div className="space-y-6">
        <Card className={`glass-card transition-all duration-300 ${
          hasPendingChanges 
            ? 'border-yellow-500/50 bg-yellow-500/5' 
            : 'border-red-500/30 bg-red-500/5'
        }`}>
          <CardHeader>
            <CardTitle className={`flex items-center ${
              hasPendingChanges ? 'text-yellow-400' : 'text-red-400'
            }`}>
              <AlertTriangle className="w-5 h-5 mr-2" />
              Service Management
              {hasPendingChanges && (
                <Badge className="ml-2 bg-yellow-500/20 text-yellow-300 border-yellow-500/30">
                  Pending Changes
                </Badge>
              )}
            </CardTitle>
            <CardDescription className={hasPendingChanges ? 'text-yellow-300' : 'text-red-300'}>
              {hasPendingChanges 
                ? 'You have unsaved changes. Click "Save Changes" to apply them or "Revert" to discard.'
                : 'Enable or disable services permanently. This will affect the availability of services system-wide.'}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            {serviceConfigLoading ? (
              <div className="space-y-4">
                <Skeleton className="h-6 w-full" />
                <Skeleton className="h-6 w-full" />
              </div>
            ) : (
              <div className="space-y-6">
                <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                  <div className={`p-4 border rounded-lg transition-all ${
                    hasPendingChanges 
                      ? 'border-yellow-500/20 bg-yellow-500/10' 
                      : 'border-red-500/20 bg-red-500/10'
                  }`}>
                    <div className="flex items-center space-x-3 mb-3">
                      <Checkbox
                        id="enable_wallet_discovery"
                        checked={pendingServiceConfig?.enable_wallet_discovery || false}
                        onCheckedChange={(checked) => updatePendingServiceConfig('enable_wallet_discovery', !!checked)}
                        disabled={serviceConfigMutation.isLoading}
                        className={`border-red-400 data-[state=checked]:bg-red-500 data-[state=checked]:border-red-500 ${
                          hasPendingChanges && pendingServiceConfig?.enable_wallet_discovery !== serviceConfig?.enable_wallet_discovery
                            ? 'ring-2 ring-yellow-400/50'
                            : ''
                        }`}
                      />
                      <Label htmlFor="enable_wallet_discovery" className="text-white font-medium">
                        Enable Wallet Discovery Service
                      </Label>
                    </div>
                    <p className={`text-sm ml-6 ${
                      hasPendingChanges ? 'text-yellow-300' : 'text-red-300'
                    }`}>
                      {pendingServiceConfig?.enable_wallet_discovery 
                        ? "✅ Service will be enabled and can be started"
                        : "❌ Service will be disabled and cannot be started"}
                    </p>
                  </div>
                  
                  <div className={`p-4 border rounded-lg transition-all ${
                    hasPendingChanges 
                      ? 'border-yellow-500/20 bg-yellow-500/10' 
                      : 'border-red-500/20 bg-red-500/10'
                  }`}>
                    <div className="flex items-center space-x-3 mb-3">
                      <Checkbox
                        id="enable_pnl_analysis"
                        checked={pendingServiceConfig?.enable_pnl_analysis || false}
                        onCheckedChange={(checked) => updatePendingServiceConfig('enable_pnl_analysis', !!checked)}
                        disabled={serviceConfigMutation.isLoading}
                        className={`border-red-400 data-[state=checked]:bg-red-500 data-[state=checked]:border-red-500 ${
                          hasPendingChanges && pendingServiceConfig?.enable_pnl_analysis !== serviceConfig?.enable_pnl_analysis
                            ? 'ring-2 ring-yellow-400/50'
                            : ''
                        }`}
                      />
                      <Label htmlFor="enable_pnl_analysis" className="text-white font-medium">
                        Enable P&L Analysis Service
                      </Label>
                    </div>
                    <p className={`text-sm ml-6 ${
                      hasPendingChanges ? 'text-yellow-300' : 'text-red-300'
                    }`}>
                      {pendingServiceConfig?.enable_pnl_analysis 
                        ? "✅ Service will be enabled and can be started"
                        : "❌ Service will be disabled and cannot be started"}
                    </p>
                  </div>
                </div>
                
                <div className={`flex items-center justify-between p-4 border rounded-lg transition-all ${
                  hasPendingChanges 
                    ? 'border-yellow-500/20 bg-yellow-500/5' 
                    : 'border-red-500/20 bg-red-500/5'
                }`}>
                  <div>
                    <p className="text-white font-medium">
                      {hasPendingChanges ? 'Apply Configuration Changes' : 'Service Configuration'}
                    </p>
                    <p className={`text-sm ${
                      hasPendingChanges ? 'text-yellow-300' : 'text-red-300'
                    }`}>
                      {hasPendingChanges 
                        ? 'You have unsaved changes that need to be applied'
                        : 'Changes will be applied immediately and affect service availability'}
                    </p>
                  </div>
                  <div className="flex items-center space-x-3">
                    {hasPendingChanges && (
                      <Button
                        onClick={handleRevertChanges}
                        disabled={serviceConfigMutation.isLoading}
                        variant="outline"
                        className="border-gray-500/50 text-gray-400 hover:bg-gray-500/20"
                      >
                        Revert
                      </Button>
                    )}
                    <Button
                      onClick={handleServiceConfigSave}
                      disabled={serviceConfigMutation.isLoading || !hasPendingChanges}
                      variant="destructive"
                      className={`${
                        hasPendingChanges 
                          ? 'bg-yellow-600 hover:bg-yellow-700' 
                          : 'bg-red-600 hover:bg-red-700'
                      }`}
                    >
                      {serviceConfigMutation.isLoading ? 'Saving...' : 
                       hasPendingChanges ? 'Save Changes' : 'No Changes'}
                    </Button>
                  </div>
                </div>
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Confirmation Dialog */}
      <Dialog open={confirmDialog.open} onOpenChange={(open) => setConfirmDialog({ ...confirmDialog, open })}>
        <DialogContent className="glass-card border-blue-ice/20">
          <DialogHeader>
            <DialogTitle className="text-white flex items-center">
              <AlertTriangle className="w-5 h-5 mr-2 text-orange-warning" />
              Confirm Action
            </DialogTitle>
            <DialogDescription className="text-gray-400">
              Are you sure you want to {confirmDialog.action} the {confirmDialog.service.replace('_', ' ')} service?
              {confirmDialog.action === 'stop' && ' This will interrupt any ongoing processing.'}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setConfirmDialog({ ...confirmDialog, open: false })}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={confirmDialog.onConfirm}
              disabled={serviceControlMutation.isLoading}
            >
              {confirmDialog.action === 'stop' ? 'Stop Service' : 'Restart Service'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}